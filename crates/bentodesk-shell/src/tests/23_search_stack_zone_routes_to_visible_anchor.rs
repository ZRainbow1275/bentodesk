#[test]
fn search_hidden_stack_member_moves_highlight_from_anchor_to_focused_preview() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(41), "Anchor", 900, 500, 520, 420));
        app.zones
            .add(Zone::new(ZoneId(42), "Hidden Member", 930, 520, 480, 360));
        assert!(app.zones.stack(ZoneId(41), ZoneId(42)));
    }
    assert!(super::run_search_query(&root, "hidden member") > 0);

    assert!(super::activate_search_hit(
        &root,
        "zone:42",
        core::ptr::null_mut(),
    ));

    let mut commands = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut commands), 1);
    assert!(matches!(
        commands.first(),
        Some(Command::OpenStackTray(ZoneId(42)))
    ));
    let app = root.app.borrow();
    assert_ne!(app.selected_zone.get(), Some(ZoneId(42)));
    assert_eq!(app.hovered_zone.get(), Some(ZoneId(41)));
    let anchor = app.zones.get(ZoneId(41)).expect("anchor");
    let expected = app.zone_collapsed_rect(anchor);
    let overlay = app.highlight_overlay.borrow();
    assert_eq!(overlay.targets().len(), 1);
    assert_eq!(overlay.targets()[0].to_rect(), expected);
    drop(overlay);
    drop(app);

    root.dispatcher.push(commands.remove(0));
    consume_dispatcher(&root, core::ptr::null_mut());
    let app = root.app.borrow();
    let state = app.stack_tray.borrow().clone().expect("stack tray");
    assert_eq!(state.selected_member_id, ZoneId(42));
    let anchor = app.zones.get(ZoneId(41)).expect("anchor");
    let tray = stack_tray::stack_tray_rect(app.viewport, anchor, 2);
    let preview = stack_tray::focused_preview_rect(app.viewport, tray);
    let overlay = app.highlight_overlay.borrow();
    assert_eq!(overlay.targets().len(), 1);
    assert_eq!(overlay.targets()[0].to_rect(), preview);
}

#[test]
fn hidden_stack_member_item_search_routes_to_focused_tray() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1280.0,
            height: 720.0,
        };
        app.zones
            .add(Zone::new(ZoneId(51), "Anchor", 900, 500, 520, 420));
        let mut child = Zone::new(ZoneId(52), "Child", 930, 520, 480, 360);
        child.items.push(ZoneItem::new(
            ZoneItemId(7),
            r"Z:\does-not-exist\Hidden Contract.txt",
            "",
            0,
            0,
        ));
        app.zones.add(child);
        assert!(app.zones.stack(ZoneId(51), ZoneId(52)));
        assert_eq!(
            super::stacked_owner_for_search_hit(&app, "item:52:7"),
            Some(ZoneId(52))
        );
    }
    assert!(super::run_search_query(&root, "hidden contract") > 0);

    assert!(super::activate_search_hit(
        &root,
        "item:52:7",
        core::ptr::null_mut(),
    ));

    let mut commands = smallvec::SmallVec::<[Command; 8]>::new();
    assert_eq!(root.dispatcher.drain_into(&mut commands), 1);
    assert!(matches!(
        commands.first(),
        Some(Command::OpenStackTray(ZoneId(52)))
    ));
}

#[test]
fn suggestor_hidden_stack_member_path_targets_visible_anchor_not_desktop() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 1280.0,
        height: 720.0,
    };
    app.zones
        .add(Zone::new(ZoneId(61), "Anchor", 900, 500, 520, 420));
    let path = r"C:\Desktop\Hidden Contract.txt";
    let mut child = Zone::new(ZoneId(62), "Child", 930, 520, 480, 360);
    child
        .add_item(Cow::Borrowed(path), Cow::Borrowed("hash"))
        .expect("item");
    app.zones.add(child);
    assert!(app.zones.stack(ZoneId(61), ZoneId(62)));

    assert!(super::path_matches_visible_zone_item(&app, path));
    let targets = super::highlight_targets_for_paths(&app, &[path.to_owned()]);
    let anchor = app.zones.get(ZoneId(61)).expect("anchor");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].to_rect(), app.zone_collapsed_rect(anchor));
}

#[test]
fn free_zone_search_and_keyboard_activation_expand_in_all_modes_and_quadrants() {
    let quadrants = [
        ("left-top", 20, 20, false, false),
        ("right-top", 1_040, 20, true, false),
        ("left-bottom", 20, 760, false, true),
        ("right-bottom", 1_040, 760, true, true),
    ];

    for mode in [
        ZoneDisplayMode::Hover,
        ZoneDisplayMode::Click,
        ZoneDisplayMode::Always,
    ] {
        for (quadrant, x, y, anchor_right, anchor_bottom) in quadrants {
            for activation in ["search", "keyboard"] {
                let root = test_app_root();
                {
                    let mut app = root.app.borrow_mut();
                    app.viewport = Size {
                        width: 1_200.0,
                        height: 900.0,
                    };
                    app.zones
                        .add(Zone::new(ZoneId(71), "Activation Target", x, y, 320, 240));
                    app.zones
                        .add(Zone::new(ZoneId(99), "Other Zone", 600, 400, 240, 180));
                    app.set_zone_display_mode(mode);
                }

                match activation {
                    "search" => {
                        assert!(super::run_search_query(&root, "activation target") > 0);
                        assert!(super::activate_search_hit(
                            &root,
                            "zone:71",
                            core::ptr::null_mut(),
                        ));
                    }
                    "keyboard" => assert!(focus_visible_zone(&root, true)),
                    _ => unreachable!(),
                }

                let app = root.app.borrow();
                let zone = app.zones.get(ZoneId(71)).expect("free zone");
                assert_eq!(
                    app.selected_zone.get(),
                    Some(zone.id),
                    "{mode:?} {quadrant} {activation}"
                );
                assert_eq!(
                    app.hovered_zone.get(),
                    Some(zone.id),
                    "{mode:?} {quadrant} {activation}"
                );
                assert!(
                    app.zone_pill_body_visible(zone),
                    "{mode:?} {quadrant} {activation}"
                );
                assert_eq!(
                    app.explicit_zone_surface_hold.get(),
                    Some(zone.id),
                    "{mode:?} {quadrant} {activation}"
                );

                let hold_now =
                    unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
                drive_hover_scheduler(&app, None, hold_now);
                assert!(
                    !poll_hover_scheduler(&app, hold_now.wrapping_add(5_000)),
                    "{mode:?} {quadrant} {activation} must stay expanded away from the pointer"
                );
                assert!(app.zone_pill_body_visible(zone));
                drive_hover_scheduler(&app, Some(ZoneId(99)), hold_now.wrapping_add(5_001));
                assert_eq!(app.explicit_zone_surface_hold.get(), Some(zone.id));
                assert!(app.zone_pill_body_visible(zone));

                let capsule = app.zone_collapsed_rect(zone);
                let placement = app.zone_expanded_placement(zone);
                assert_eq!(
                    placement.anchor_right, anchor_right,
                    "{mode:?} {quadrant} {activation}"
                );
                assert_eq!(
                    placement.anchor_bottom, anchor_bottom,
                    "{mode:?} {quadrant} {activation}"
                );
                if anchor_right {
                    assert!((placement.panel.right() - capsule.right()).abs() < 0.01);
                } else {
                    assert!((placement.panel.x - capsule.x).abs() < 0.01);
                }
                if anchor_bottom {
                    assert!((placement.panel.bottom() - capsule.bottom()).abs() < 0.01);
                } else {
                    assert!((placement.panel.y - capsule.y).abs() < 0.01);
                }
                assert!(placement.panel.x >= 0.0 && placement.panel.y >= 0.0);
                assert!(placement.panel.right() <= app.viewport.width);
                assert!(placement.panel.bottom() <= app.viewport.height);

                let morph = app.zone_pill_morph_at(zone.id, unsafe {
                    windows_sys::Win32::System::SystemInformation::GetTickCount()
                });
                if mode == ZoneDisplayMode::Always {
                    assert!(
                        morph.is_none(),
                        "Always activation must not shrink an already-open panel"
                    );
                } else {
                    assert!(
                        morph.is_some(),
                        "{mode:?} {quadrant} {activation} must start PillMorph"
                    );
                }

                drive_hover_scheduler(&app, Some(zone.id), hold_now.wrapping_add(5_002));
                assert_eq!(app.explicit_zone_surface_hold.get(), None);
            }
        }
    }
}

#[test]
fn hidden_or_removed_explicit_target_cannot_block_visible_hover_zone() {
    for remove_target in [false, true] {
        let root = test_app_root();
        let start = 10_000;
        {
            let mut app = root.app.borrow_mut();
            app.viewport = Size {
                width: 1_200.0,
                height: 900.0,
            };
            app.zones
                .add(Zone::new(ZoneId(81), "Explicit", 20, 20, 320, 240));
            app.zones
                .add(Zone::new(ZoneId(82), "Hover", 600, 400, 320, 240));
            app.set_zone_display_mode(ZoneDisplayMode::Hover);
            assert!(activate_free_zone_surface(&app, ZoneId(81), start));
            app.explicit_zone_surface_hold.set(Some(ZoneId(81)));
            if remove_target {
                assert!(app.zones.remove(ZoneId(81)));
            } else {
                assert!(
                    app.zones
                        .get_mut(ZoneId(81))
                        .expect("explicit target")
                        .set_visible(false)
                );
            }
        }

        let app = root.app.borrow();
        drive_hover_scheduler(&app, Some(ZoneId(82)), start + 1);
        assert_eq!(app.explicit_zone_surface_hold.get(), None);
        assert!(poll_hover_scheduler(&app, start + 5_000));
        assert_eq!(app.hover_scheduler.get().expanded_zone(), Some(ZoneId(82)));
    }
}

#[test]
fn search_reveals_hidden_free_zone_before_holding_its_surface() {
    let root = test_app_root();
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1_200.0,
            height: 900.0,
        };
        let mut zone = Zone::new(ZoneId(83), "Hidden Search Target", 20, 20, 320, 240);
        zone.set_visible(false);
        app.zones.add(zone);
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
    }

    assert!(super::run_search_query(&root, "hidden search target") > 0);
    assert!(super::activate_search_hit(
        &root,
        "zone:83",
        core::ptr::null_mut(),
    ));
    let app = root.app.borrow();
    assert!(app.zones.get(ZoneId(83)).expect("target").is_visible());
    assert_eq!(app.explicit_zone_surface_hold.get(), Some(ZoneId(83)));
}

#[test]
fn pointer_already_inside_explicit_target_releases_hold_then_leave_collapses() {
    let root = test_app_root();
    let start = 20_000;
    {
        let mut app = root.app.borrow_mut();
        app.viewport = Size {
            width: 1_200.0,
            height: 900.0,
        };
        app.zones
            .add(Zone::new(ZoneId(84), "Inside Target", 20, 20, 320, 240));
        app.set_zone_display_mode(ZoneDisplayMode::Hover);
        assert!(activate_free_zone_surface(&app, ZoneId(84), start));
        app.explicit_zone_surface_hold.set(Some(ZoneId(84)));
    }

    let app = root.app.borrow();
    let capsule = app.zone_collapsed_rect(app.zones.get(ZoneId(84)).expect("target"));
    assert!(update_main_zone_hover_for_point(
        &app,
        capsule.x + capsule.width * 0.5,
        capsule.y + capsule.height * 0.5,
        start + 1,
    ));
    assert_eq!(app.explicit_zone_surface_hold.get(), None);

    assert!(update_main_zone_hover_for_point(
        &app,
        app.viewport.width - 1.0,
        app.viewport.height - 1.0,
        start + 2,
    ));
    assert!(poll_hover_scheduler(&app, start + 5_000));
    assert_eq!(app.hover_scheduler.get().expanded_zone(), None);
}

#[test]
fn four_quadrant_search_and_suggestor_highlights_use_effective_panel() {
    for (name, x, y) in [
        ("left-top", 20, 20),
        ("right-top", 1_040, 20),
        ("left-bottom", 20, 760),
        ("right-bottom", 1_040, 760),
    ] {
        for mode in [
            ZoneDisplayMode::Hover,
            ZoneDisplayMode::Click,
            ZoneDisplayMode::Always,
        ] {
            let root = test_app_root();
            let path = format!(r"C:\Desktop\{name}-{mode:?}-contract.txt");
            {
                let mut app = root.app.borrow_mut();
                app.viewport = Size {
                    width: 1_200.0,
                    height: 900.0,
                };
                let mut zone = Zone::new(ZoneId(72), "Highlight Target", x, y, 320, 240);
                zone.add_item(path.clone(), "hash").expect("item");
                app.zones.add(zone);
                app.set_zone_display_mode(mode);
            }

            assert!(super::run_search_query(&root, "highlight target") > 0);
            assert!(super::activate_search_hit(
                &root,
                "zone:72",
                core::ptr::null_mut(),
            ));
            let app = root.app.borrow();
            let zone = app.zones.get(ZoneId(72)).expect("zone");
            let panel = app.zone_expanded_placement(zone).panel;
            let search_targets = app.highlight_overlay.borrow();
            assert_eq!(search_targets.targets().len(), 1, "{name} {mode:?} search");
            assert_eq!(
                search_targets.targets()[0].to_rect(),
                panel,
                "{name} {mode:?} search"
            );
            drop(search_targets);

            if mode == ZoneDisplayMode::Always {
                let suggestor_targets = super::highlight_targets_for_paths(&app, &[path]);
                let expected = app
                    .resolve_zone_item_flow_layout(zone, panel, 0.0, zone.items.iter())
                    .card_for(zone.items[0].id)
                    .map(|card| {
                        bentodesk_app::business::highlight_overlay::HighlightRect::from_rect(
                            card.rect,
                        )
                    })
                    .expect("shared responsive highlight card");
                assert_eq!(
                    suggestor_targets.as_slice(),
                    &[expected],
                    "{name} suggestor"
                );
            }
        }
    }
}

#[test]
fn click_release_activation_starts_morph_after_pointer_down_preselects_zone() {
    let mut app = AppState::new();
    app.zones
        .add(Zone::new(ZoneId(81), "Click target", 40, 40, 320, 240));
    app.set_zone_display_mode(ZoneDisplayMode::Click);
    // Main pointer-down selects before pointer-up decides this was a click.
    app.selected_zone.set(Some(ZoneId(81)));
    app.hovered_zone.set(Some(ZoneId(81)));

    assert!(activate_free_zone_surface(&app, ZoneId(81), 1_000));
    assert_eq!(app.hover_scheduler.get().expanded_zone(), Some(ZoneId(81)));
    assert!(app.zone_pill_morph_at(ZoneId(81), 1_001).is_some());
}
