#[test]
fn m6c_unknown_id_leaves_effect_none() {
    // Applying an unknown (custom JSON) id is rejected and leaves the
    // dark-default `None` effect untouched.
    use bento_nano_style::tokens::EffectTauri;
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("shell-purple"), None);
    assert_eq!(app.active_theme_effect_tauri(), EffectTauri::None);
}

#[test]
fn m6b_order_yields_sharp_radius_via_accessor() {
    // The former documented partial is gone: applying `order` (a non-
    // registry theme) now yields its real per-theme card radius (6), not
    // the dark/light default.
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("order"), Some(true));
    assert_eq!(app.active_theme_radius_tauri().card, 6.0);
    assert_eq!(app.active_theme_radius_tauri().capsule, 8.0);
}

#[test]
fn m6b_terminal_font_flows_into_family2_typo() {
    // The font-swap path reads `active_theme_typography()` (Family-2). After
    // M6b that returns Consolas for terminal (closing the partial).
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("terminal"), Some(true));
    assert_eq!(
        app.active_theme_typography().font_family.as_str(),
        "Consolas"
    );
    assert_eq!(app.active_theme_typography_tauri().font_family, "Consolas");
    // editorial → Georgia, and its widget radius collapses to sharp 0.
    assert_eq!(app.apply_active_theme_by_id("editorial"), Some(true));
    assert_eq!(
        app.active_theme_typography().font_family.as_str(),
        "Georgia"
    );
    assert_eq!(app.active_theme_radius().xl.top_left, 0.0);
}

#[test]
fn m6b_brutalism_flattens_family2_widget_shadow() {
    // Angular `none` themes flatten the widget-chrome shadow.
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("brutalism"), Some(true));
    assert_eq!(app.active_theme_shadow().md, bento_nano_style::Shadow::NONE);
    assert!(app.active_theme_shadow_tauri().zen.is_empty());
}

#[test]
fn m6b_dark_family2_stays_byte_identical() {
    // §5.3 net: dark's Family-2 tokens must equal DARK_DEFAULT exactly.
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
    assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
    assert_eq!(
        app.active_theme_radius(),
        bento_nano_theme::DARK_DEFAULT.radius
    );
    assert_eq!(
        app.active_theme_shadow(),
        bento_nano_theme::DARK_DEFAULT.shadow
    );
    assert_eq!(
        app.active_theme_typography().font_family,
        bento_nano_theme::DARK_DEFAULT.typo.font_family,
    );
}

#[test]
fn apply_unknown_id_returns_none_and_leaves_theme_unchanged() {
    let app = AppState::new();
    assert_eq!(app.apply_active_theme_by_id("shell-purple"), None);
    // Untouched — still the dark default.
    assert_eq!(
        app.active_theme_tauri(),
        bento_nano_style::tokens::PALETTE_DARK,
    );
    assert_eq!(app.active_theme_id.borrow().as_str(), "dark");
}

/// α5 (S2, 2026-05-24) — pin AppState invariant that `theme_base_accent`
/// defaults to `None`. Combined with the call-site removal in
/// `render.rs::render_frame` (the unconditional `draw_theme_base_accent`
/// at line 470 was deleted), a fresh launch no longer paints the 4-DIP
/// accent strip over the desktop. If a future regression resurrects that
/// paint call AND leaves `theme_base_accent = None`, the strip falls
/// back to `palette.accent × 0.92` — exactly the blue strip the user
/// reported. This test is the canary for the state half of the contract;
/// the call-site half is pinned by the comment + git history at
/// render.rs:470.
#[test]
fn theme_base_accent_defaults_to_none_alpha_s2_regression_pin() {
    let app = AppState::new();
    assert!(
        app.theme_base_accent.borrow().is_none(),
        "α5 S2 regression: theme_base_accent must default to None so a \
             future re-introduced top strip paint at least falls through the \
             swatch picker rather than the palette fallback. Setting a \
             default here would re-paint the leaked top strip the moment the \
             call site comes back."
    );
}

#[test]
fn zone_content_scroll_is_bounded_to_its_current_zone() {
    let app = AppState::new();
    assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 0.0);

    assert!(app.set_zone_content_scroll(ZoneId(4), 86.0));
    assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 86.0);
    assert_eq!(app.zone_content_scroll_offset(ZoneId(5)), 0.0);
    assert!(!app.set_zone_content_scroll(ZoneId(4), 86.0));

    assert!(app.set_zone_content_scroll(ZoneId(5), f32::INFINITY));
    assert_eq!(app.zone_content_scroll_offset(ZoneId(4)), 0.0);
    assert_eq!(app.zone_content_scroll_offset(ZoneId(5)), 0.0);
    assert!(!app.reset_zone_content_scroll());
}

#[test]
fn inline_zone_search_progress_has_stable_open_and_animated_states() {
    let app = AppState::new();
    let zone_id = ZoneId(4);
    assert_eq!(app.zone_search_animation_progress_at(100), 0.0);

    app.zone_search_target.set(Some(zone_id));
    assert_eq!(app.zone_search_animation_progress_at(100), 1.0);

    app.pill_animator.borrow_mut().start(
        zone_id,
        AnimChannel::InlineSearch,
        100,
        180,
        0.0,
        1.0,
        crate::animator::Easing::EaseOutCubic,
    );
    assert_eq!(app.zone_search_animation_progress_at(100), 0.0);
    assert!(app.zone_search_animation_progress_at(190) > 0.5);
    assert_eq!(app.zone_search_animation_progress_at(280), 1.0);

    app.zone_search_closing.set(true);
    app.pill_animator
        .borrow_mut()
        .cancel(zone_id, AnimChannel::InlineSearch);
    assert_eq!(app.zone_search_animation_progress_at(300), 0.0);
}

#[test]
fn active_inline_search_holds_hover_zone_open_until_target_clears() {
    let mut app = AppState::new();
    let zone_id = ZoneId(4);
    app.zones
        .add(Zone::new(zone_id, "Search", 10, 20, 240, 180));
    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    let zone = app.zones.get(zone_id).expect("zone");

    assert!(!app.zone_pill_body_visible(zone));
    app.zone_search_target.set(Some(zone_id));
    assert!(app.zone_pill_body_visible(zone));
    app.zone_search_target.set(None);
    assert!(!app.zone_pill_body_visible(zone));
}

#[test]
fn zone_pill_anim_defaults_are_settled() {
    // Wave G2 — fresh AppState must report no pill morph in flight so
    // the renderer's morph branch stays dormant until hover starts one.
    let app = AppState::new();
    assert_eq!(app.zone_pill_anim_zone.get(), None);
    assert_eq!(app.zone_pill_anim_started_ms.get(), 0);
    assert!((app.zone_pill_anim_progress.get() - 1.0).abs() < f32::EPSILON);
    assert!((app.zone_pill_anim_from_morph.get() - 0.0).abs() < f32::EPSILON);
    assert_eq!(
        app.zone_pill_anim_duration_ms.get(),
        crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
    );
    assert!(!app.zone_pill_anim_expanding.get());
}

#[test]
fn zone_body_visibility_respects_hover_always_click() {
    let app = AppState::new();
    let zone = Zone::new(ZoneId(7), Cow::Borrowed("docs"), 10, 10, 160, 120);

    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    assert!(!app.zone_body_visible_for_mode(&zone));
    app.hovered_zone.set(Some(zone.id));
    assert!(!app.zone_body_visible_for_mode(&zone));
    {
        let mut scheduler = app.hover_scheduler.get();
        scheduler.mark_expanded(zone.id, 100);
        app.hover_scheduler.set(scheduler);
    }
    assert!(app.zone_body_visible_for_mode(&zone));
    app.hovered_zone.set(None);
    assert!(app.zone_body_visible_for_mode(&zone));
    {
        let mut scheduler = app.hover_scheduler.get();
        scheduler.reset();
        app.hover_scheduler.set(scheduler);
    }
    assert!(!app.zone_body_visible_for_mode(&zone));
    app.selected_zone.set(Some(zone.id));
    assert!(
        !app.zone_body_visible_for_mode(&zone),
        "ordinary clicks must not expand a hover-mode Zone"
    );

    app.selected_zone.set(None);
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    assert!(!app.zone_body_visible_for_mode(&zone));
    app.selected_zone.set(Some(zone.id));
    assert!(app.zone_body_visible_for_mode(&zone));

    app.selected_zone.set(None);
    app.set_zone_display_mode(ZoneDisplayMode::Always);
    assert!(app.zone_body_visible_for_mode(&zone));
}

#[test]
fn changing_display_mode_cancels_stale_hover_intent_and_morph() {
    let app = AppState::new();
    let mut scheduler = app.hover_scheduler.get();
    scheduler.on_enter(ZoneId(7), 100, 150);
    app.hover_scheduler.set(scheduler);
    app.selected_zone.set(Some(ZoneId(7)));
    app.zone_pill_anim_zone.set(Some(ZoneId(7)));
    app.zone_pill_anim_progress.set(0.4);
    app.zone_pill_anim_expanding.set(true);

    assert!(app.set_zone_display_mode(ZoneDisplayMode::Click));
    assert_eq!(app.selected_zone.get(), None);
    assert!(!app.hover_scheduler.get().is_pending());
    assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
    assert_eq!(app.zone_pill_anim_zone.get(), None);
    assert_eq!(app.zone_pill_anim_progress.get(), 1.0);
    assert!(!app.zone_pill_anim_expanding.get());
}

#[test]
fn click_selection_does_not_leak_into_restored_hover_mode() {
    let app = AppState::new();
    let zone = Zone::new(ZoneId(7), Cow::Borrowed("docs"), 10, 10, 160, 120);

    assert!(app.set_zone_display_mode(ZoneDisplayMode::Click));
    app.selected_zone.set(Some(zone.id));
    assert!(app.zone_body_visible_for_mode(&zone));

    assert!(app.set_zone_display_mode(ZoneDisplayMode::Always));
    assert_eq!(app.selected_zone.get(), None);
    assert!(app.zone_body_visible_for_mode(&zone));

    assert!(app.set_zone_display_mode(ZoneDisplayMode::Hover));
    assert_eq!(app.selected_zone.get(), None);
    assert!(!app.zone_body_visible_for_mode(&zone));
}

#[test]
fn zone_pill_morph_in_flight_keeps_both_start_frames_on_top() {
    let app = AppState::new();
    let zone = Zone::new(ZoneId(10), Cow::Borrowed("docs"), 10, 10, 160, 120);

    app.zone_pill_anim_zone.set(Some(zone.id));
    app.zone_pill_anim_expanding.set(true);
    app.zone_pill_anim_progress.set(0.0);
    assert!(app.zone_pill_morph_in_flight(&zone));
    assert!(app.zone_on_top(&zone));

    app.zone_pill_anim_progress.set(0.25);
    assert!(app.zone_pill_morph_in_flight(&zone));
    assert!(app.zone_on_top(&zone));

    app.zone_pill_anim_expanding.set(false);
    app.zone_pill_anim_progress.set(0.0);
    assert!(app.zone_pill_morph_in_flight(&zone));
    assert!(app.zone_on_top(&zone));

    app.zone_pill_anim_progress.set(1.0);
    assert!(!app.zone_pill_morph_in_flight(&zone));
    assert!(!app.zone_on_top(&zone));
}

#[test]
fn zone_drag_from_collapsed_pill_suppresses_mouse_down_selection_expand() {
    let app = AppState::new();
    let zone = Zone::new(ZoneId(8), Cow::Borrowed("docs"), 10, 10, 160, 120);

    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    assert!(!app.zone_pill_body_visible(&zone));

    app.selected_zone.set(Some(zone.id));
    app.zone_drag.set(Some((zone.id, 4, 4)));
    app.zone_drag_body_visible_at_start
        .set(Some((zone.id, false)));

    assert!(!app.zone_pill_body_visible(&zone));
    assert!(!app.zone_on_top(&zone));
}

#[test]
fn zone_drag_from_expanded_body_collapses_to_capsule() {
    let app = AppState::new();
    let zone = Zone::new(ZoneId(9), Cow::Borrowed("docs"), 10, 10, 160, 120);

    app.set_zone_display_mode(ZoneDisplayMode::Hover);
    let mut scheduler = app.hover_scheduler.get();
    scheduler.mark_expanded(zone.id, 100);
    app.hover_scheduler.set(scheduler);
    assert!(app.zone_pill_body_visible(&zone));

    app.zone_drag.set(Some((zone.id, 4, 4)));
    app.zone_drag_body_visible_at_start
        .set(Some((zone.id, true)));

    assert!(!app.zone_pill_body_visible(&zone));
    assert!(!app.zone_on_top(&zone));
}

/// M1a 2026-05-29 — `snapshot_settings`/`restore_settings` are the single
/// round-trip surface the Settings panel's Cancel/Escape/Close × path uses
/// to undo unsaved General-section edits. Set all 5 toggle Cells to
/// non-default values, snapshot, scribble different values, then restore
/// and assert every Cell is back to the snapshotted value. Also pins that
/// `settings_dirty` is `false` on a fresh AppState (Save dims until a row
/// is mutated — Tauri `disabled={!dirty()}`).
#[test]
fn settings_snapshot_restore_round_trips_general_toggles() {
    let app = AppState::new();

    assert!(
        !app.settings_dirty.get(),
        "settings_dirty must default to false so Save starts dimmed"
    );

    // Defaults are embed=on, autostart=off, taskbar=on, smart=on,
    // portable=off. Pick the inverse of each so a no-op snapshot can't
    // pass by accident.
    app.setting_desktop_embed.set(false);
    app.setting_autostart.set(true);
    app.setting_show_in_taskbar.set(false);
    app.setting_smart_layout.set(false);
    app.setting_portable_mode.set(true);
    // M1d — set the 9 Performance/Startup fields to non-default values too.
    app.expand_delay_ms.set(200);
    app.collapse_delay_ms.set(400);
    app.icon_cache_size.set(900);
    app.startup_high_priority.set(true);
    app.crash_restart_enabled.set(false);
    app.crash_max_retries.set(7);
    app.crash_window_secs.set(45);
    app.safe_start_after_hibernation.set(false);
    app.hibernate_resume_delay_ms.set(3500);
    assert_eq!(app.apply_active_theme_by_id("ocean-blue"), Some(true));
    app.zone_display_mode.set(ZoneDisplayMode::Click);
    // W2 (#7 fix wave) — set the two §2 Paths drafts to non-default values
    // so the snapshot/restore round-trip is exercised for them too.
    *app.desktop_path_draft.borrow_mut() = SmolStr::new("E:\\Custom\\Desktop");
    *app.watch_paths_draft.borrow_mut() = SmolStr::new("E:\\Watch\\A\nE:\\Watch\\B");

    let snap = app.snapshot_settings();
    assert_eq!(
        snap,
        SettingsSnapshot {
            ghost_layer_enabled: false,
            launch_at_startup: true,
            show_in_taskbar: false,
            auto_group_enabled: false,
            portable_mode: true,
            expand_delay_ms: 200,
            collapse_delay_ms: 400,
            icon_cache_size: 900,
            startup_high_priority: true,
            crash_restart_enabled: false,
            crash_max_retries: 7,
            crash_window_secs: 45,
            safe_start_after_hibernation: false,
            hibernate_resume_delay_ms: 3500,
            active_theme_id: SmolStr::new_static("ocean-blue"),
            zone_display_mode: ZoneDisplayMode::Click,
            desktop_path_draft: SmolStr::new("E:\\Custom\\Desktop"),
            watch_paths_draft: SmolStr::new("E:\\Watch\\A\nE:\\Watch\\B"),
        }
    );

    // Mutate every Cell away from the snapshot (simulate cancelled edits).
    app.setting_desktop_embed.set(true);
    app.setting_autostart.set(false);
    app.setting_show_in_taskbar.set(true);
    app.setting_smart_layout.set(true);
    app.setting_portable_mode.set(false);
    app.expand_delay_ms.set(50);
    app.collapse_delay_ms.set(100);
    app.icon_cache_size.set(100);
    app.startup_high_priority.set(false);
    app.crash_restart_enabled.set(true);
    app.crash_max_retries.set(1);
    app.crash_window_secs.set(5);
    app.safe_start_after_hibernation.set(true);
    app.hibernate_resume_delay_ms.set(500);
    assert_eq!(app.apply_active_theme_by_id("dark"), Some(true));
    app.zone_display_mode.set(ZoneDisplayMode::Always);
    *app.desktop_path_draft.borrow_mut() = SmolStr::new("Z:\\scribbled");
    *app.watch_paths_draft.borrow_mut() = SmolStr::new("Z:\\scribbled\nZ:\\again");

    app.restore_settings(&snap);

    assert!(!app.setting_desktop_embed.get());
    assert!(app.setting_autostart.get());
    assert!(!app.setting_show_in_taskbar.get());
    assert!(!app.setting_smart_layout.get());
    assert!(app.setting_portable_mode.get());
    // M1d — the 9 new fields round-trip through snapshot → restore.
    assert_eq!(app.expand_delay_ms.get(), 200);
    assert_eq!(app.collapse_delay_ms.get(), 400);
    assert_eq!(app.icon_cache_size.get(), 900);
    assert!(app.startup_high_priority.get());
    assert!(!app.crash_restart_enabled.get());
    assert_eq!(app.crash_max_retries.get(), 7);
    assert_eq!(app.crash_window_secs.get(), 45);
    assert!(!app.safe_start_after_hibernation.get());
    assert_eq!(app.hibernate_resume_delay_ms.get(), 3500);
    assert_eq!(app.active_theme_id.borrow().as_str(), "ocean-blue");
    assert_eq!(app.zone_display_mode.get(), ZoneDisplayMode::Click);
    // W2 — the two §2 Paths drafts round-trip through snapshot → restore.
    assert_eq!(
        app.desktop_path_draft.borrow().as_str(),
        "E:\\Custom\\Desktop"
    );
    assert_eq!(
        app.watch_paths_draft.borrow().as_str(),
        "E:\\Watch\\A\nE:\\Watch\\B"
    );
}

/// M1a 2026-05-29 — the Cancel/Escape/Close × path stashes the snapshot in
/// `settings_snapshot: RefCell<Option<SettingsSnapshot>>` when the panel
/// opens and `take()`s it on restore. Pin that the container round-trips:
/// it starts `None`, holds the stored value, and reads back `None` after a
/// `take()` so a second restore can't replay a stale snapshot.
#[test]
fn settings_snapshot_cell_round_trips_through_refcell_option() {
    let app = AppState::new();
    assert!(app.settings_snapshot.borrow().is_none());

    let snap = SettingsSnapshot {
        ghost_layer_enabled: false,
        launch_at_startup: true,
        show_in_taskbar: false,
        auto_group_enabled: true,
        portable_mode: true,
        expand_delay_ms: DEFAULT_EXPAND_DELAY_MS,
        collapse_delay_ms: DEFAULT_COLLAPSE_DELAY_MS,
        icon_cache_size: 500,
        startup_high_priority: false,
        crash_restart_enabled: true,
        crash_max_retries: 3,
        crash_window_secs: 60,
        safe_start_after_hibernation: true,
        hibernate_resume_delay_ms: 2000,
        active_theme_id: SmolStr::new_static("dark"),
        zone_display_mode: ZoneDisplayMode::Hover,
        desktop_path_draft: SmolStr::new("D:\\Desktop"),
        watch_paths_draft: SmolStr::default(),
    };
    // W2 — `SettingsSnapshot` is no longer `Copy` (it carries two `SmolStr`
    // drafts), so clone into the slot and compare against a clone.
    app.settings_snapshot.borrow_mut().replace(snap.clone());
    assert_eq!(app.settings_snapshot.borrow().as_ref(), Some(&snap));

    let taken = app.settings_snapshot.borrow_mut().take();
    assert_eq!(taken, Some(snap));
    assert!(
        app.settings_snapshot.borrow().is_none(),
        "after take() the slot must be empty so cancel can't replay a stale snapshot"
    );
}

/// M1d 2026-05-29 — `slider_fraction_to_value` maps a track fraction to a
/// stepped, clamped value. Pin the endpoints + snapping for each of the 4
/// slider ranges so a drag can never produce an off-grid / out-of-range
/// value. (Tauri min/max/step from `SettingsPanel.tsx:601-698`.)
#[test]
fn m1d_slider_fraction_clamps_and_snaps_to_step() {
    // Expand delay 50..500 step 10.
    assert_eq!(
        slider_fraction_to_value(
            0.0,
            EXPAND_DELAY_MIN_MS,
            EXPAND_DELAY_MAX_MS,
            EXPAND_DELAY_STEP_MS
        ),
        50
    );
    assert_eq!(
        slider_fraction_to_value(
            1.0,
            EXPAND_DELAY_MIN_MS,
            EXPAND_DELAY_MAX_MS,
            EXPAND_DELAY_STEP_MS
        ),
        500
    );
    // Below 0 / above 1 saturate at the endpoints (never out of range).
    assert_eq!(
        slider_fraction_to_value(
            -5.0,
            EXPAND_DELAY_MIN_MS,
            EXPAND_DELAY_MAX_MS,
            EXPAND_DELAY_STEP_MS
        ),
        50
    );
    assert_eq!(
        slider_fraction_to_value(
            9.0,
            EXPAND_DELAY_MIN_MS,
            EXPAND_DELAY_MAX_MS,
            EXPAND_DELAY_STEP_MS
        ),
        500
    );
    // Midpoint snaps to the nearest 10-step. (50 + 0.5*450 = 275 → 280).
    let mid = slider_fraction_to_value(
        0.5,
        EXPAND_DELAY_MIN_MS,
        EXPAND_DELAY_MAX_MS,
        EXPAND_DELAY_STEP_MS,
    );
    assert_eq!(mid % EXPAND_DELAY_STEP_MS, 0, "value must snap to step");
    assert!((EXPAND_DELAY_MIN_MS..=EXPAND_DELAY_MAX_MS).contains(&mid));

    // Collapse delay 100..1000 step 50 — every output is a 50-multiple.
    for n in 0..=10 {
        let f = n as f32 / 10.0;
        let v = slider_fraction_to_value(
            f,
            COLLAPSE_DELAY_MIN_MS,
            COLLAPSE_DELAY_MAX_MS,
            COLLAPSE_DELAY_STEP_MS,
        );
        assert_eq!((v - COLLAPSE_DELAY_MIN_MS) % COLLAPSE_DELAY_STEP_MS, 0);
        assert!((COLLAPSE_DELAY_MIN_MS..=COLLAPSE_DELAY_MAX_MS).contains(&v));
    }

    // Icon cache 100..2000 step 100.
    assert_eq!(
        slider_fraction_to_value(0.0, ICON_CACHE_MIN, ICON_CACHE_MAX, ICON_CACHE_STEP),
        100
    );
    assert_eq!(
        slider_fraction_to_value(1.0, ICON_CACHE_MIN, ICON_CACHE_MAX, ICON_CACHE_STEP),
        2000
    );

    // Hibernate delay 500..5000 step 100.
    assert_eq!(
        slider_fraction_to_value(
            0.0,
            HIBERNATE_DELAY_MIN_MS,
            HIBERNATE_DELAY_MAX_MS,
            HIBERNATE_DELAY_STEP_MS
        ),
        500
    );
    assert_eq!(
        slider_fraction_to_value(
            1.0,
            HIBERNATE_DELAY_MIN_MS,
            HIBERNATE_DELAY_MAX_MS,
            HIBERNATE_DELAY_STEP_MS
        ),
        5000
    );

    // step <= 0 degrades to a plain clamp (panic-free), never out of range.
    assert_eq!(slider_fraction_to_value(0.5, 1, 10, 0), 6);
    assert_eq!(slider_fraction_to_value(-1.0, 1, 10, 0), 1);
    assert_eq!(slider_fraction_to_value(2.0, 1, 10, 0), 10);
}

/// Performance/startup controls seed valid values. Pin the release motion
/// defaults as well as their ranges so first-run response cannot silently
/// drift back to the slower reference cadence.
#[test]
fn m1d_perf_startup_defaults_in_range() {
    let app = AppState::new();
    assert_eq!(DEFAULT_EXPAND_DELAY_MS, 60);
    assert_eq!(DEFAULT_COLLAPSE_DELAY_MS, 150);
    assert!(
        DEFAULT_EXPAND_DELAY_MS as u32 + crate::zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS
            <= 400
    );
    assert_eq!(app.expand_delay_ms.get(), DEFAULT_EXPAND_DELAY_MS);
    assert_eq!(app.collapse_delay_ms.get(), DEFAULT_COLLAPSE_DELAY_MS);
    assert!((EXPAND_DELAY_MIN_MS..=EXPAND_DELAY_MAX_MS).contains(&app.expand_delay_ms.get()));
    assert!((COLLAPSE_DELAY_MIN_MS..=COLLAPSE_DELAY_MAX_MS).contains(&app.collapse_delay_ms.get()));
    assert!((ICON_CACHE_MIN..=ICON_CACHE_MAX).contains(&app.icon_cache_size.get()));
    assert!((CRASH_MAX_RETRIES_MIN..=CRASH_MAX_RETRIES_MAX).contains(&app.crash_max_retries.get()));
    assert!((CRASH_WINDOW_SECS_MIN..=CRASH_WINDOW_SECS_MAX).contains(&app.crash_window_secs.get()));
    assert!(
        (HIBERNATE_DELAY_MIN_MS..=HIBERNATE_DELAY_MAX_MS)
            .contains(&app.hibernate_resume_delay_ms.get())
    );
    // Bounds match Tauri exactly.
    assert_eq!((CRASH_MAX_RETRIES_MIN, CRASH_MAX_RETRIES_MAX), (1, 10));
    assert_eq!((CRASH_WINDOW_SECS_MIN, CRASH_WINDOW_SECS_MAX), (5, 60));
}
