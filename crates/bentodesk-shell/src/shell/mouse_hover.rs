//! Native shell owner: `mouse_hover`.

use super::*;

pub(super) fn handle_mouse_move(root: &AppRoot, slot: &WindowSlot, x: f32, y: f32) {
    // #5 drag motion arbitration (2026-06-08) — once a normal zone/resize/item
    // drag is armed, the captured pointer owns the move stream. Apply live
    // drag geometry before any hover/tooltip producer can retarget morph,
    // bloom, or item-hover channels. Persistence still waits until mouse-up.
    if handle_active_pointer_drag(root, slot, x, y) {
        return;
    }
    if root.app.borrow().active_context_menu.borrow().is_some() {
        handle_context_menu_mouse_move(root, slot.hwnd, x, y);
        return;
    }
    {
        let app = root.app.borrow();
        if slot.kind == WindowKind::Search {
            if let Some(command) = tooltip_command_for_search_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::IconPicker {
            if let Some(command) = tooltip_command_for_icon_picker_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::PalettePicker {
            if let Some(command) = tooltip_command_for_palette_picker_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::CapsulePicker {
            if let Some(command) = tooltip_command_for_capsule_picker_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::ZoneEditor {
            if let Some(command) = tooltip_command_for_zone_editor_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::ItemFileRename {
            if let Some(command) = tooltip_command_for_item_file_rename_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::Suggestor {
            if let Some(command) = tooltip_command_for_suggestor_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::BulkManager {
            if let Some(command) = tooltip_command_for_bulk_manager_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::RulesWizard {
            if let Some(command) = tooltip_command_for_rules_wizard_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::Timeline {
            if let Some(command) = tooltip_command_for_timeline_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::SnapshotPicker {
            if let Some(command) = tooltip_command_for_snapshot_picker_hover(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if slot.kind == WindowKind::MiniBar {
            let viewport = window_slot_logical_viewport(slot);
            if let Some(command) = tooltip_command_for_minibar_hover(
                &app,
                viewport,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                x,
                y,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        let settings_open = app.settings_open.get();
        let settings_pointer_active = settings_open
            && window_kind_routes_settings_pointer(slot.kind, settings_aux_registered(root));
        if settings_pointer_active {
            drop(app);
            sync_app_viewport_from_window_slot(root, slot);
            let app = root.app.borrow();
            let hit = ui::settings_hit(&app, x, y);
            let encryption_hover_target = settings_encryption_mode_hover_for_hit(hit);
            let appearance_hover_target = settings_appearance_hover_for_hit(hit);
            let close_hover_target = settings_close_hover_for_hit(hit);
            let hover_changed = update_settings_encryption_mode_hover_for_hit(&app, hit)
                | update_settings_appearance_hover_for_hit(&app, hit)
                | update_settings_close_hover_for_hit(&app, hit);
            if animation_proof_log_enabled() {
                log_static(
                    format!(
                        "settings_hover: x={x:.1} y={y:.1} viewport={:.1}x{:.1} hit={hit:?} encryption_hover={encryption_hover_target:?} appearance_hover={appearance_hover_target:?} close_hover={close_hover_target} changed={hover_changed}\n",
                        app.viewport.width,
                        app.viewport.height,
                    )
                    .as_str(),
                );
            }
            if hover_changed {
                request_redraw(slot.hwnd);
            }
            if let Some(command) = tooltip_command_for_settings_hit(
                &app,
                bentodesk_app::WindowHandle(slot.hwnd as isize),
                hit,
            ) {
                root.dispatcher.push(command);
            }
            return;
        }
        if settings_open {
            return;
        }
        if let Some(command) = tooltip_command_for_main_hover(
            &app,
            &slot.state,
            bentodesk_app::WindowHandle(slot.hwnd as isize),
            x,
            y,
        ) {
            root.dispatcher.push(command);
        }
        if update_panel_header_button_hover(&app, x, y) {
            request_redraw(slot.hwnd);
        }
        // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
        let now_ms = unsafe { GetTickCount() };
        if update_main_zone_hover_for_point(&app, x, y, now_ms) {
            // The shared update also tracks petal-to-petal movement while the
            // stack anchor itself stays unchanged, so hover intent cannot be
            // skipped merely because both petals map to the same ZoneId.
            arm_hover_frame_timer(slot.hwnd);
            request_redraw(slot.hwnd);
        }
        // M3-A2 (2026-05-29) — per-item hover scale. Runs on EVERY move (not
        // gated on the zone change above) because moving between cards inside
        // the same expanded zone must re-target the ramp. Suppressed while an
        // item drag is in flight (the grid being reordered shouldn't hover-pop)
        // — `update_item_hover_animator` clears the hovered card in that case.
        if update_item_hover_animator(&app, x, y) {
            request_redraw(slot.hwnd);
        }
    }

    // Hover retargeting — touches the widget tree, so the borrow_mut here is
    // legitimately app-state-only (no zone mutation).
    let mut app = root.app.borrow_mut();
    let hit = ui::hit_test(&slot.state, x, y);
    let mut prev = root.hovered.borrow_mut();
    if *prev != hit {
        if let Some(old) = *prev {
            if ui::is_icon_button(&app, old) {
                if let Ok(WidgetNode::IconButton(btn)) = app.tree.get_mut(old) {
                    btn.set_hovered(false);
                }
            }
        }
        if let Some(new) = hit {
            if ui::is_icon_button(&app, new) {
                if let Ok(WidgetNode::IconButton(btn)) = app.tree.get_mut(new) {
                    btn.set_hovered(true);
                }
            }
        }
        *prev = hit;
    }
}

pub(super) fn refresh_ghost_cursor_passthrough(
    root: &AppRoot,
    slot: &WindowSlot,
    hwnd: HWND,
) -> Option<(f32, f32, bool)> {
    if slot.kind != WindowKind::Main {
        return None;
    }

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos writes to the provided POINT and ScreenToClient
    // converts it for the live main HWND. Both calls are best-effort; failure
    // leaves passthrough unchanged for this tick.
    unsafe {
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        if ScreenToClient(hwnd, &mut point) == 0 {
            return None;
        }
    }
    let dpi = slot.state.dpi.get();
    let x = bentodesk_style::dpi::device_to_logical_f32(point.x as f32, dpi);
    let y = bentodesk_style::dpi::device_to_logical_f32(point.y as f32, dpi);
    let passthrough = apply_ghost_cursor_passthrough_for_point(root, slot, x, y);
    // V-7 (2026-05-21): the prior `SetWindowRgn` carve-out around the cursor
    // produced a visible square "mask" that tracked pointer movement — the OS
    // hard-clips outside the region, so the 48 DIP hole read as a moving
    // bitmap artefact. Wave H1's documented design only needs the
    // `WS_EX_TRANSPARENT` toggle (already applied above by
    // `apply_ghost_cursor_passthrough_for_point` → `set_cursor_passthrough`)
    // to route blank-pixel clicks to Explorer; no region surgery is required.
    Some((x, y, passthrough))
}

pub(super) fn should_clear_stale_main_hover(
    has_hover: bool,
    pointer_drag_active: bool,
    cursor_window: HWND,
    main_window: HWND,
) -> bool {
    has_hover && !pointer_drag_active && cursor_window != main_window
}

pub(super) fn reconcile_main_hover_from_cursor(
    root: &AppRoot,
    slot: &WindowSlot,
    hwnd: HWND,
) -> bool {
    if slot.kind != WindowKind::Main {
        return false;
    }
    let (has_hover, pointer_drag_active) = {
        let app = root.app.borrow();
        (
            app.hovered_zone.get().is_some(),
            normal_pointer_drag_active(&app),
        )
    };
    if !has_hover || pointer_drag_active {
        return false;
    }

    let mut screen_point = POINT { x: 0, y: 0 };
    // SAFETY: both APIs only inspect the current desktop cursor/window state.
    if unsafe { GetCursorPos(&mut screen_point) } == 0 {
        return false;
    }
    let cursor_window = unsafe { WindowFromPoint(screen_point) };
    if should_clear_stale_main_hover(has_hover, pointer_drag_active, cursor_window, hwnd) {
        clear_hover(root);
        arm_hover_frame_timer(hwnd);
        return true;
    }

    let mut client_point = screen_point;
    // SAFETY: `hwnd` is the live Main HWND currently owning `slot`.
    if unsafe { ScreenToClient(hwnd, &mut client_point) } == 0 {
        return false;
    }
    let dpi = slot.state.dpi.get();
    let x = bentodesk_style::dpi::device_to_logical_f32(client_point.x as f32, dpi);
    let y = bentodesk_style::dpi::device_to_logical_f32(client_point.y as f32, dpi);
    let actual_hover = {
        let app = root.app.borrow();
        stack_aware_hover_zone_for_point(&app, x, y)
    };
    let current_hover = root.app.borrow().hovered_zone.get();
    if current_hover == actual_hover {
        return false;
    }
    handle_mouse_move(root, slot, x, y);
    true
}

pub(super) fn apply_ghost_cursor_passthrough_for_point(
    root: &AppRoot,
    slot: &WindowSlot,
    x: f32,
    y: f32,
) -> bool {
    if slot.kind != WindowKind::Main {
        return false;
    }
    let passthrough = {
        let app = root.app.borrow();
        matches!(
            ui::main_nchittest_kind(&app, &slot.state, x, y),
            ui::HitKind::Transparent
        )
    };
    bentodesk_backend::ghost_layer::set_cursor_passthrough(passthrough);
    passthrough
}

pub(super) fn clear_hover(root: &AppRoot) {
    let mut app = root.app.borrow_mut();
    // SAFETY: GetTickCount is total + thread-safe.
    let now_ms = unsafe { GetTickCount() };
    let should_hide_tooltip = app.active_tooltip.borrow().is_some();
    if normal_pointer_drag_active(&app) {
        let dragged_zone = app.zone_drag.get().map(|(id, _, _)| id);
        reset_pointer_drag_hover_channels(&app, dragged_zone, now_ms);
        app.set_panel_header_button_hover(None);
        drop(app);
        if should_hide_tooltip {
            root.dispatcher.push(Command::HideTooltip);
        }
        return;
    }
    // V-8 — release any in-flight hover micro-animation before we clobber
    // `hovered_zone`. Mirrors the mouse-move path.
    update_pill_hover_animator(&app, None, now_ms);
    // M3-A2 — pointer left the overlay: clear the hovered card so its 150ms
    // hover-out ramp runs (the just-left slot animates back to identity).
    {
        let mut state = app.item_hover.get();
        let _ = state.on_hover(None, now_ms);
        app.item_hover.set(state);
    }
    app.hovered_zone.set(None);
    app.set_panel_header_button_hover(None);
    app.set_settings_encryption_mode_hover(None);
    app.set_settings_close_hover(false);
    update_stack_bloom_hover(&app, None, now_ms);
    {
        let mut interaction = app.stack_bloom_interaction.get();
        if interaction.active_member.is_some()
            && interaction.active_member_leave_started_ms.is_none()
        {
            interaction.active_member_leave_started_ms = Some(now_ms);
            interaction.hover_preview_opened = true;
            app.stack_bloom_interaction.set(interaction);
        }
    }
    // A3 (2026-05-29) — pointer left the overlay. Do NOT collapse the open
    // zone instantly; arm the grace-collapse so a transient twitch off a pill
    // (or a brief WS_EX_TRANSPARENT passthrough flicker) doesn't drop the
    // zone before the user returns. The former 80ms `LEAVE_GRACE_MS` dead
    // const is now LIVE as the user-tunable `collapse_delay_ms` consumed
    // inside `drive_hover_scheduler`. The per-frame `poll_hover_scheduler`
    // fires the actual collapse morph once the grace elapses (HOVER mode).
    drive_hover_scheduler(&app, None, now_ms);
    let mut prev = root.hovered.borrow_mut();
    if let Some(old) = *prev {
        if ui::is_icon_button(&app, old) {
            if let Ok(WidgetNode::IconButton(btn)) = app.tree.get_mut(old) {
                btn.set_hovered(false);
            }
        }
    }
    *prev = None;
    drop(prev);
    drop(app);
    if should_hide_tooltip {
        root.dispatcher.push(Command::HideTooltip);
    }
}
