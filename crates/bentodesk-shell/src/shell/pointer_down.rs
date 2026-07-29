//! Native shell owner: `pointer_down`.

use super::*;

pub(super) fn handle_lbutton_down(root: &AppRoot, slot: &WindowSlot, hwnd: HWND, x: f32, y: f32) {
    if root.app.borrow().active_context_menu.borrow().is_some() {
        return;
    }
    if should_ignore_main_pointer_while_settings_aux_open(root, slot.kind) {
        return;
    }
    let settings_aux_registered = settings_aux_registered(root);
    let settings_pointer_active = root.app.borrow().settings_open.get()
        && window_kind_routes_settings_pointer(slot.kind, settings_aux_registered);
    if settings_pointer_active {
        sync_app_viewport_from_window_slot(root, slot);
    }
    let app = root.app.borrow();

    if app.about_open.get() && matches!(slot.kind, WindowKind::Main | WindowKind::About) {
        let viewport = window_slot_logical_viewport(slot);
        let about_hit = bentodesk_app::business::about::hit_test(viewport, x, y);
        drop(app);
        match about_hit {
            bentodesk_app::business::about::AboutHit::Close
            | bentodesk_app::business::about::AboutHit::Outside => {
                root.dispatcher.push(Command::CloseAbout);
            }
            bentodesk_app::business::about::AboutHit::Project => {
                if let Err(code) = shell_execute_path(
                    "open",
                    bentodesk_app::business::about::PROJECT_URL_FULL,
                    None,
                ) {
                    log_static(format!("about: open_project failed code={code}\n").as_str());
                }
            }
            bentodesk_app::business::about::AboutHit::Author => {
                if let Err(code) =
                    shell_execute_path("open", bentodesk_app::business::about::GITHUB_URL, None)
                {
                    log_static(format!("about: open_author failed code={code}\n").as_str());
                }
            }
            bentodesk_app::business::about::AboutHit::Body => {}
        }
        return;
    }

    if settings_pointer_active {
        drop(app);
        handle_settings_lbutton_down(root, hwnd, x, y);
        return;
    }

    if handle_stack_bloom_preview_lbutton_down(&app, hwnd, x, y) {
        return;
    }

    if handle_stack_tray_lbutton_down(root, x, y) {
        return;
    }

    let clicked_zone = ui::hit_test_zone(&app, x, y);
    let clicked_zone_is_stack_anchor = clicked_zone
        .and_then(|id| app.zones.get(id))
        .is_some_and(|zone| zone.is_stack_anchor());
    let clicked_zone_body_visible_before_select = clicked_zone
        .and_then(|id| {
            app.zones
                .get(id)
                .map(|zone| app.zone_pill_body_visible(zone))
        })
        .unwrap_or(false);
    let selected_zone_before_mouse_down = app.selected_zone.get();
    if clicked_zone_is_stack_anchor {
        // `StackCapsule.tsx` owns click/hover itself. Selecting the anchor here
        // would incorrectly replace the compact capsule with a full Zone panel.
        app.selected_zone.set(None);
    } else if app.selected_zone.get() != clicked_zone {
        app.selected_zone.set(clicked_zone);
    }

    if let Some(id) = ui::hit_test(&slot.state, x, y)
        && ui::is_icon_button(&app, id)
    {
        let event_id = match app.tree.get(id) {
            Ok(WidgetNode::IconButton(btn)) => btn.on_click_event,
            _ => return,
        };
        push_button_command(root, event_id);
        return;
    }

    if let Some(hit) = ui::hit_test_inline_zone_search(&app, x, y) {
        match hit {
            ui::InlineZoneSearchHit::Clear => {
                app.search_bar.borrow_mut().clear();
                app.reset_zone_content_scroll();
            }
            ui::InlineZoneSearchHit::Body => set_main_inline_search_keyboard_focus(hwnd, true),
        }
        // SAFETY: GetTickCount is total and thread-safe.
        touch_inline_zone_search(&app, unsafe { GetTickCount() });
        drop(app);
        request_redraw(hwnd);
        return;
    }

    if let Some((zone_id, item_id, path)) = ui::hit_test_zone_item(&app, x, y) {
        if drag_proof_log_enabled() {
            log_static(
                format!(
                    "items: drag-proof lbutton_down item zone_id={} item_id={} x={x:.1} y={y:.1} path={path}\n",
                    zone_id.0,
                    item_id.0
                )
                .as_str(),
            );
        }
        app.item_drag.borrow_mut().replace(ItemDragCandidate {
            zone_id,
            item_id,
            path: smol_str::SmolStr::new(path),
            start_x: x as i32,
            start_y: y as i32,
            last_x: x as i32,
            last_y: y as i32,
            is_internal_dragging: false,
        });
        // M3-A2 — fire the item press-down ramp toward Tauri's `:active`
        // scale(0.97). The release half runs from the global mouse-up path
        // (`release_item_press_animator`) so a drag-off still tidies the
        // press. Mirrors the pill `start_pill_press_animator` contract.
        // SAFETY: GetTickCount is total + thread-safe.
        let now_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() };
        start_item_press_animator(&app, zone_id, item_id, now_ms);
        unsafe { SetCapture(hwnd) };
        return;
    }
    if drag_proof_log_enabled() {
        log_static(format!("items: drag-proof lbutton_down no_item x={x:.1} y={y:.1}\n").as_str());
    }

    // GROUP-4 (2026-06-01, 1:1) — the expanded `PanelHeader` search + close
    // action buttons. Checked BEFORE the resize-corner / zone-drag paths so a
    // click on a 28× 28 button never falls through to a zone drag. Tauri
    // `PanelHeader.tsx`: search → `openSearch(zone.id)`; close → `onClose()`
    // (collapses the panel back to its pill).
    if let Some((zone_id, button)) = ui::hit_test_zone_header_button(&app, x, y) {
        match button {
            ui::HeaderButton::Search => {
                let already_open =
                    app.zone_search_target.get() == Some(zone_id) && !app.zone_search_closing.get();
                drop(app);
                if already_open {
                    close_inline_zone_search(root, hwnd);
                } else {
                    open_inline_zone_search(root, zone_id, hwnd);
                }
                return;
            }
            ui::HeaderButton::Close => {
                // Collapse through one path that also clears the mouse-down
                // selection; otherwise selected_zone forces the body visible
                // again as soon as the morph settles.
                // SAFETY: GetTickCount is total + thread-safe.
                let now_ms = unsafe { GetTickCount() };
                let closes_search = app.zone_search_target.get() == Some(zone_id);
                if closes_search {
                    drop(app);
                    close_inline_zone_search(root, hwnd);
                    let app = root.app.borrow();
                    collapse_zone_from_header(&app, zone_id, now_ms);
                    drop(app);
                    request_redraw(hwnd);
                    return;
                }
                collapse_zone_from_header(&app, zone_id, now_ms);
                drop(app);
                request_redraw(hwnd);
                return;
            }
        }
    }

    if let Some(id) = ui::hit_test_zone_resize_corner(&app, x, y) {
        if let Some(z) = app.zones.get(id) {
            // M4 locked gate — a locked zone cannot resize (Tauri parity:
            // BentoZone.tsx:1198 `if (zoneLocked()) return;`). Selection on
            // mouse-down (above) still applies; we just don't arm zone_resize.
            if z.locked {
                return;
            }
            app.zone_resize.set(Some((id, z.w, z.h)));
            // SAFETY: SetCapture canonical.
            unsafe { SetCapture(hwnd) };
        }
        return;
    }
    if let Some(id) = ui::hit_test_zone(&app, x, y)
        && let Some(z) = app.zones.get(id)
    {
        // M4 locked gate — a locked zone cannot drag, move, or become a
        // stack member via drag (Tauri parity: BentoZone.tsx:852
        // `if (zoneLocked()) return;`). Because zone_drag is never armed,
        // the move handler pushes no MoveZone and the mouse-up F2 stack
        // search short-circuits (was_drag == false). One gate covers move
        // AND stack. Selection (set above) is unaffected.
        if z.locked {
            return;
        }
        // Tauri v8 centers the painted zen/stack capsule under the
        // pointer once drag latches. Persisted `w/h` are expanded-panel
        // dimensions and the click position inside that panel must not
        // become the drag offset.
        let Some((dx, dy)) = zone_drag_pointer_offset(&app, id) else {
            return;
        };
        app.zone_drag.set(Some((id, dx, dy)));
        app.zone_drag_body_visible_at_start
            .set(Some((id, clicked_zone_body_visible_before_select)));
        app.zone_drag_selected_before_start
            .set(selected_zone_before_mouse_down);
        // M4 — capture the mouse-down origin so the move handler can gate
        // MoveZone behind the 4-DIP drag threshold (moved = false until
        // the pointer travels past it). Tuple = (start_x, start_y, moved).
        app.zone_drag_origin.set(Some((x as i32, y as i32, false)));
        // SAFETY: GetTickCount is total + thread-safe.
        let now_ms = unsafe { GetTickCount() };
        // Keep an already-open Bloom alive for a sub-threshold capsule
        // click so mouse-up can toggle it. A real drag still clears every
        // hover channel at the threshold latch in `handle_active_pointer_drag`.
        if !z.is_stack_anchor() {
            reset_pointer_drag_hover_channels(&app, Some(id), now_ms);
        }
        // V-8 — fire press-down animator unless this is a stack anchor
        // (which paints via its own chrome and doesn't run the V-8 path).
        if !z.is_stack_anchor() {
            start_pill_press_animator(&app, id, now_ms);
        }
        // SAFETY: SetCapture canonical.
        unsafe { SetCapture(hwnd) };
        // Production reader for `monitors` cache.
        let _ = bentodesk_platform::zone_active_monitor_index(z, &slot.state.monitors);
    }
}
