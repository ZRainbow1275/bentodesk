//! Native shell owner: `input_core`.

use super::*;

// -----------------------------------------------------------------------------
// Hotkey routing.
// -----------------------------------------------------------------------------

pub(super) fn auxiliary_escape_action(kind: WindowKind, vk: u32) -> Option<AuxiliaryEscapeAction> {
    if kind == WindowKind::Main || vk != VK_ESCAPE_KEY {
        return None;
    }
    if kind == WindowKind::About {
        Some(AuxiliaryEscapeAction::CloseAbout)
    } else {
        Some(AuxiliaryEscapeAction::HideAuxWindow)
    }
}

pub(super) fn main_window_visible(root: &AppRoot) -> bool {
    root.registry
        .borrow()
        .iter()
        .find(|slot| slot.kind == WindowKind::Main)
        .map(|slot| slot.is_visible.get())
        .unwrap_or(false)
}

pub(super) fn queue_toggle_main_window(root: &AppRoot) {
    let command = if main_window_visible(root) {
        Command::HideWindow(WindowKind::Main)
    } else {
        Command::ShowWindow(WindowKind::Main)
    };
    root.dispatcher.push(command);
}

pub(super) fn selected_or_first_zone_id(app: &AppState) -> Option<ZoneId> {
    if let Some(id) = app
        .selected_zone
        .get()
        .filter(|id| app.zones.get(*id).is_some())
    {
        return Some(id);
    }
    app.zones
        .iter()
        .find(|zone| zone.is_visible() && !zone.is_stacked_child())
        .or_else(|| app.zones.iter().next())
        .map(|zone| zone.id)
}

pub(super) fn visible_top_level_zone_ids(app: &AppState) -> Vec<ZoneId> {
    app.zones
        .iter()
        .filter(|zone| zone.is_visible() && !zone.is_stacked_child())
        .map(|zone| zone.id)
        .collect()
}

pub(super) fn queue_toggle_selected_zone_lock(root: &AppRoot) {
    let update = {
        let app = root.app.borrow();
        let Some(id) = selected_or_first_zone_id(&app) else {
            return;
        };
        let Some(zone) = app.zones.get(id) else {
            return;
        };
        BulkZoneUpdate {
            id,
            locked: Some(!zone.locked),
            ..BulkZoneUpdate::default()
        }
    };
    root.dispatcher.push(Command::BulkUpdateZones(vec![update]));
}

pub(super) fn queue_toggle_all_zones_visible(root: &AppRoot) {
    let (ids, visible) = {
        let app = root.app.borrow();
        let ids: Vec<_> = app.zones.iter().map(|zone| zone.id).collect();
        if ids.is_empty() {
            return;
        }
        let show_all = !app.zones.iter().any(|zone| zone.is_visible());
        (ids, show_all)
    };
    root.dispatcher
        .push(Command::BulkSetZonesVisible { ids, visible });
}

pub(super) fn queue_reflow_visible_zones(root: &AppRoot) {
    let ids = {
        let app = root.app.borrow();
        visible_top_level_zone_ids(&app)
    };
    if ids.is_empty() {
        return;
    }
    root.dispatcher.push(Command::BulkApplyLayout {
        ids,
        algorithm: BulkLayoutAlgorithm::Grid,
    });
}

pub(super) fn duplicate_selected_zone(root: &AppRoot) -> bool {
    let mut app = root.app.borrow_mut();
    let Some(source_id) = selected_or_first_zone_id(&app) else {
        return false;
    };
    let Some(source) = app.zones.get(source_id).cloned() else {
        return false;
    };
    let mut duplicate = source;
    let new_id = app.alloc_zone_id();
    let max_x = ((app.viewport.width as i32) - duplicate.w).max(0);
    let max_y = ((app.viewport.height as i32) - duplicate.h).max(0);
    duplicate.id = new_id;
    duplicate.title = Cow::Owned(format!("{} *", duplicate.title.as_ref()));
    duplicate.x = duplicate.x.saturating_add(24).clamp(0, max_x);
    duplicate.y = duplicate.y.saturating_add(24).clamp(0, max_y);
    duplicate.visible = true;
    duplicate.locked = false;
    duplicate.alias = None;
    duplicate.live_folder_path = None;
    duplicate.stack_parent = None;
    duplicate.stack_members.clear();
    duplicate.items.clear();
    app.zones.add(duplicate);
    app.selected_zone.set(Some(new_id));
    app.hovered_zone.set(Some(new_id));
    app.mark_dirty();
    true
}

pub(super) fn focus_visible_zone(root: &AppRoot, forward: bool) -> bool {
    let app = root.app.borrow();
    let ids = visible_top_level_zone_ids(&app);
    if ids.is_empty() {
        return false;
    }
    let current = app.selected_zone.get();
    let current_index = current.and_then(|id| ids.iter().position(|candidate| *candidate == id));
    let next_index = match (current_index, forward) {
        (Some(index), true) => (index + 1) % ids.len(),
        (Some(0), false) => ids.len() - 1,
        (Some(index), false) => index - 1,
        (None, true) => 0,
        (None, false) => ids.len() - 1,
    };
    let next = ids[next_index];
    app.selected_zone.set(Some(next));
    app.hovered_zone.set(Some(next));
    true
}

pub(super) fn dispatch_hotkey_command(root: &AppRoot, command: hotkey::HotkeyCommand) {
    match command {
        hotkey::HotkeyCommand::Escape => {
            let settings_open = root.app.borrow().settings_open.get();
            if settings_open {
                // M1a 2026-05-29 — Escape dismisses the Settings panel
                // without persisting any pending General-section edits,
                // matching Tauri's `handleClose` keyboard branch
                // (`SettingsPanel.tsx:165-169`).
                cancel_settings_general(root);
                root.dispatcher.push(Command::CloseSettings);
            } else {
                root.dispatcher.push(Command::HideWindow(WindowKind::Main));
            }
        }
        hotkey::HotkeyCommand::ToggleMain => {
            queue_toggle_main_window(root);
        }
        hotkey::HotkeyCommand::CreateZone => {
            root.dispatcher
                .push(Command::CreateZone(default_zone_spec(root)));
        }
        hotkey::HotkeyCommand::DuplicateZone => {
            root.dispatcher.push(Command::DuplicateZone);
        }
        hotkey::HotkeyCommand::ToggleZoneLock => {
            root.dispatcher.push(Command::ToggleSelectedZoneLock);
        }
        hotkey::HotkeyCommand::ToggleAllZones => {
            root.dispatcher.push(Command::ToggleAllZonesVisible);
        }
        hotkey::HotkeyCommand::AutoOrganize => {
            root.dispatcher.push(Command::AutoOrganize);
        }
        hotkey::HotkeyCommand::ReflowLayout => {
            root.dispatcher.push(Command::ReflowVisibleZones);
        }
        hotkey::HotkeyCommand::OpenBulkManager => {
            root.dispatcher.push(Command::OpenBulkManager);
        }
        hotkey::HotkeyCommand::FocusNextZone => {
            root.dispatcher.push(Command::FocusNextZone);
        }
        hotkey::HotkeyCommand::FocusPreviousZone => {
            root.dispatcher.push(Command::FocusPreviousZone);
        }
        hotkey::HotkeyCommand::ToggleSettings => {
            root.dispatcher.push(Command::ToggleSettings);
        }
        hotkey::HotkeyCommand::OpenSearch => {
            root.dispatcher.push(Command::OpenSearch);
        }
        hotkey::HotkeyCommand::QuitApp => {
            root.dispatcher.push(Command::QuitApp);
        }
        hotkey::HotkeyCommand::OpenTimeline => {
            root.dispatcher.push(Command::OpenTimeline);
        }
        hotkey::HotkeyCommand::OpenSnapshotPicker => {
            root.dispatcher.push(Command::OpenSnapshotPicker);
        }
        hotkey::HotkeyCommand::UndoCheckpoint => {
            root.dispatcher.push(Command::UndoCheckpoint);
        }
        hotkey::HotkeyCommand::RedoCheckpoint => {
            root.dispatcher.push(Command::RedoCheckpoint);
        }
    }
}

/// W3 (#7 fix wave 2026-06-01) — the keybinding-recording / §2 text-field / §10
/// passphrase keydowns must reach BOTH the Main HWND and the focusable Settings
/// AUX HWND (which holds focus after `SetForegroundWindow`), or typing into a
/// focused settings field does nothing. The stack-tray keydown stays Main-only
/// (see `window_kind_routes_stack_tray_keydown`). Pure predicate so the routing
/// intent is unit-testable.
pub(super) fn window_kind_routes_settings_keydown(kind: WindowKind) -> bool {
    matches!(kind, WindowKind::Main | WindowKind::Settings)
}

pub(super) fn window_kind_routes_settings_pointer(
    kind: WindowKind,
    settings_aux_registered: bool,
) -> bool {
    matches!(kind, WindowKind::Settings)
        || matches!(kind, WindowKind::Main) && !settings_aux_registered
}

pub(super) fn settings_aux_registered(root: &AppRoot) -> bool {
    root.registry.borrow().count_kind(WindowKind::Settings) > 0
}

pub(super) fn should_ignore_main_pointer_while_settings_aux_open(
    root: &AppRoot,
    kind: WindowKind,
) -> bool {
    matches!(kind, WindowKind::Main)
        && root.app.borrow().settings_open.get()
        && settings_aux_registered(root)
}

pub(super) fn logical_viewport_from_device_size(
    width: u32,
    height: u32,
    dpi: u32,
) -> bentodesk_style::Size {
    bentodesk_style::dpi::device_size_to_logical(
        bentodesk_style::Size {
            width: width.max(1) as f32,
            height: height.max(1) as f32,
        },
        dpi,
    )
}

pub(super) fn window_slot_logical_viewport(slot: &WindowSlot) -> bentodesk_style::Size {
    logical_viewport_from_device_size(
        slot.renderer.width,
        slot.renderer.height,
        slot.state.dpi.get(),
    )
}

/// Run one auxiliary-window input dispatch against that HWND's own logical
/// viewport, then restore the caller's viewport.
///
/// `AppState` predates the selected-stack multi-HWND shell and still carries
/// one shared `viewport`. Paint already projects each auxiliary renderer into
/// its own DIPs; input must use the same projection or the visible control and
/// its hit target diverge whenever Main and the auxiliary window differ in
/// size. The scoped restore also prevents an editor/settings click from
/// poisoning later Main hit-testing.
pub(super) fn with_app_viewport<R>(
    root: &AppRoot,
    viewport: bentodesk_style::Size,
    callback: impl FnOnce() -> R,
) -> R {
    let previous = {
        let mut app = root.app.borrow_mut();
        let previous = app.viewport;
        app.viewport = viewport;
        previous
    };
    let result = callback();
    root.app.borrow_mut().viewport = previous;
    result
}

pub(super) fn with_window_slot_viewport<R>(
    root: &AppRoot,
    slot: &WindowSlot,
    callback: impl FnOnce() -> R,
) -> R {
    with_app_viewport(root, window_slot_logical_viewport(slot), callback)
}

pub(super) fn sync_app_viewport_from_window_slot(
    root: &AppRoot,
    slot: &WindowSlot,
) -> bentodesk_style::Size {
    let viewport = window_slot_logical_viewport(slot);
    root.app.borrow_mut().viewport = viewport;
    viewport
}

pub(super) fn settings_wheel_scroll_delta_from_wparam(wparam: WPARAM) -> Option<i32> {
    let wheel_delta = ((wparam >> 16) & 0xFFFF) as u16 as i16 as i32;
    (wheel_delta != 0).then_some(-wheel_delta)
}

pub(super) fn handle_settings_scroll_delta(root: &AppRoot, hwnd: HWND, delta: i32) {
    use bentodesk_app::settings_panel::{SettingsBodyFlags, settings_clamp_scroll};

    let app = root.app.borrow();
    let vp = app.viewport;
    let (stealth_has_retry, stealth_has_error) = match &*app.stealth_status.borrow() {
        Some(status) => (status.retry_count > 0, status.last_error.is_some()),
        None => (false, false),
    };
    let updater_kind = bentodesk_app::business::settings::updater_card::updater_height_kind(
        &app.settings_updater_status.borrow(),
    );
    let backup_visible = {
        let entries = app.settings_backup_entries.borrow();
        bentodesk_app::business::settings::backup_card::backup_visible_row_count(&entries)
    };
    let plugin_visible = {
        let entries = app.settings_plugin_entries.borrow();
        bentodesk_app::business::settings::plugins_section::plugin_visible_row_count(&entries)
    };
    let flags = SettingsBodyFlags::new(
        app.crash_restart_enabled.get(),
        app.safe_start_after_hibernation.get(),
        stealth_has_retry,
        stealth_has_error,
        updater_kind,
    )
    .with_source_rows(app.desktop_sources.borrow().len())
    .with_backup_rows(backup_visible)
    .with_backup_status(app.settings_backup_status.borrow().is_some())
    .with_encryption_status(app.settings_encryption_status.borrow().is_some())
    .with_plugin_rows(plugin_visible)
    .with_plugin_status(app.settings_plugin_status.borrow().is_some());
    let next = settings_clamp_scroll(app.scroll_offset_y.get(), delta as f32, vp, &flags);
    app.scroll_offset_y.set(next);
    drop(app);
    log_static(format!("settings: scroll delta={delta} offset={next:.1}\n").as_str());
    request_redraw(hwnd);
}

pub(super) fn handle_settings_mousewheel(
    root: &AppRoot,
    slot: &WindowSlot,
    hwnd: HWND,
    wparam: WPARAM,
) -> bool {
    let settings_open = root.app.borrow().settings_open.get();
    if !settings_open
        || !window_kind_routes_settings_pointer(slot.kind, settings_aux_registered(root))
    {
        return false;
    }
    sync_app_viewport_from_window_slot(root, slot);
    let Some(delta) = settings_wheel_scroll_delta_from_wparam(wparam) else {
        return true;
    };
    handle_settings_scroll_delta(root, hwnd, delta);
    true
}

pub(super) fn zone_item_max_scroll(app: &AppState, zone: &bentodesk_zone::Zone) -> f32 {
    let search_active = app.zone_search_target.get() == Some(zone.id);
    let item_top_offset = if search_active {
        search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX
    } else {
        0.0
    };
    if search_active {
        let query = app.search_bar.borrow();
        highlight_overlay::item_flow_max_scroll(
            zone,
            item_top_offset,
            zone.items
                .iter()
                .filter(|item| {
                    search_bar::zone_item_matches_query(item.name.as_ref(), query.query.as_str())
                })
                .map(|item| item.is_wide),
        )
    } else {
        highlight_overlay::item_flow_max_scroll(
            zone,
            item_top_offset,
            zone.items.iter().map(|item| item.is_wide),
        )
    }
}

pub(super) fn zone_scroll_target_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<(ZoneId, f32)> {
    for zone in app.zones.iter().rev() {
        if !zone.is_visible() || zone.is_stacked_child() || !app.zone_pill_body_visible(zone) {
            continue;
        }
        let search_active = app.zone_search_target.get() == Some(zone.id);
        let item_top_offset = if search_active {
            search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX
        } else {
            0.0
        };
        let clip = highlight_overlay::item_content_clip_rect(zone, item_top_offset);
        if x >= clip.x && x < clip.right() && y >= clip.y && y < clip.bottom() {
            return Some((zone.id, zone_item_max_scroll(app, zone)));
        }
    }
    None
}

pub(super) fn handle_zone_mousewheel(
    root: &AppRoot,
    slot: &WindowSlot,
    hwnd: HWND,
    wparam: WPARAM,
) -> bool {
    if slot.kind != WindowKind::Main || root.app.borrow().settings_open.get() {
        return false;
    }
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: the live Main HWND owns `slot`; both APIs only translate the
    // current desktop cursor into its client coordinate space.
    unsafe {
        if GetCursorPos(&mut point) == 0 || ScreenToClient(hwnd, &mut point) == 0 {
            return false;
        }
    }
    let dpi = slot.state.dpi.get();
    let x = bentodesk_style::dpi::device_to_logical_f32(point.x as f32, dpi);
    let y = bentodesk_style::dpi::device_to_logical_f32(point.y as f32, dpi);
    let app = root.app.borrow();
    let Some((zone_id, max_scroll)) = zone_scroll_target_for_point(&app, x, y) else {
        return false;
    };
    let Some(delta) = settings_wheel_scroll_delta_from_wparam(wparam) else {
        return true;
    };
    let current = app.zone_content_scroll_offset(zone_id).min(max_scroll);
    let next = (current + delta as f32).clamp(0.0, max_scroll);
    let changed = app.set_zone_content_scroll(zone_id, next);
    drop(app);
    if changed {
        log_static(
            format!(
                "zone: content_scroll zone={} delta={} offset={next:.1} max={max_scroll:.1}\n",
                zone_id.0, delta
            )
            .as_str(),
        );
        request_redraw(hwnd);
    }
    true
}

pub(super) fn handle_keydown(
    hwnd: HWND,
    vk: u32,
    msg: u32,
    root: &AppRoot,
    slot: &WindowSlot,
    lparam: LPARAM,
) -> LRESULT {
    if root.app.borrow().active_context_menu.borrow().is_some() {
        return handle_context_menu_keydown(root, hwnd, vk);
    }
    if slot.kind == WindowKind::ZoneEditor {
        return handle_zone_editor_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::ItemFileRename {
        return handle_item_file_rename_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::IconPicker {
        return handle_icon_picker_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::PalettePicker {
        return handle_palette_picker_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::CapsulePicker {
        return handle_capsule_picker_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::BulkManager {
        return handle_bulk_manager_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::Suggestor {
        return handle_suggestor_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::Search {
        return handle_search_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::RulesWizard {
        return handle_rules_wizard_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::Timeline {
        return handle_timeline_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::SnapshotPicker {
        return handle_snapshot_picker_keydown(root, vk, hwnd);
    }
    if slot.kind == WindowKind::Main {
        if let Some(result) = handle_inline_zone_search_keydown(root, vk, hwnd) {
            return result;
        }
    }
    // W3 (#7 fix wave 2026-06-01) — the Settings section lives on the focusable
    // Settings AUX HWND (`show_settings_surface` calls `SetForegroundWindow` on
    // it), so its keystrokes arrive here with `slot.kind == Settings`, NOT Main.
    // The keybinding-recording / §2 text-field / §10 passphrase keydowns must
    // therefore be reachable for BOTH Main and Settings or typing into a focused
    // field would do nothing (the latent pre-existing bug this fix closes). The
    // stack-tray keydown stays Main-ONLY (the tray only exists on the desktop).
    // INVARIANT: a regular Settings text field clears its focus on Esc but
    // returns `None`, so the same keydown continues to the auxiliary-escape
    // branch and closes/cancels Settings in one press. True nested captures
    // (keybinding/passphrase) still consume Esc to cancel only that capture.
    if window_kind_routes_settings_keydown(slot.kind) {
        if let Some(result) = handle_settings_keybinding_keydown(root, vk, hwnd) {
            return result;
        }
        // M1h — `handle_settings_plugins_keydown` removed: the inline Plugins
        // §11 section has no separate modal, so Esc no longer needs a plugins-
        // specific close path (panel Esc already closes the whole Settings
        // surface).
        // M7 — desktop_path / watch values live-edit keydown is tried BEFORE the
        // passphrase keydown; it returns `Some(0)` only when a non-passphrase
        // field is focused, else `None` so the passphrase + auxiliary-escape
        // paths still run.
        if let Some(result) = handle_settings_text_keydown(root, vk, hwnd) {
            return result;
        }
        if let Some(result) = handle_settings_passphrase_keydown(root, vk, hwnd) {
            return result;
        }
    }
    if slot.kind == WindowKind::Main {
        if let Some(result) = handle_stack_tray_keydown(root, vk, hwnd) {
            return result;
        }
    }
    if let Some(action) = auxiliary_escape_action(slot.kind, vk) {
        match action {
            AuxiliaryEscapeAction::CloseAbout => {
                root.dispatcher.push(Command::CloseAbout);
                consume_dispatcher(root, hwnd);
                request_redraw(hwnd);
            }
            AuxiliaryEscapeAction::HideAuxWindow => {
                // M1a 2026-05-29 — when the auxiliary HWND being dismissed
                // is the Settings panel, route through `CloseSettings` so
                // the dispatcher's close arm runs (clears `settings_open`,
                // hides the HWND) AND restore the General-section snapshot
                // first so cancelled edits never leak past Escape. Other
                // auxiliary HWNDs (live context capsule, plugin modal, etc.)
                // still take the direct ShowWindow(SW_HIDE) path.
                if slot.kind == WindowKind::Settings {
                    cancel_settings_general(root);
                    root.dispatcher.push(Command::CloseSettings);
                    consume_dispatcher(root, hwnd);
                    request_redraw(hwnd);
                } else {
                    // SAFETY: hwnd is an auxiliary HWND owned by the registry. Esc
                    // dismisses that surface instead of hiding the main window.
                    unsafe { ShowWindow(hwnd, SW_HIDE) };
                }
            }
        }
        return 0;
    }
    let mods = hotkey::ModFlags::from_keystate();
    let command = {
        let bindings = root.hotkey_bindings.borrow();
        hotkey::lookup_in(&bindings, vk, mods)
    };
    match command {
        Some(command) => {
            dispatch_hotkey_command(root, command);
            consume_dispatcher(root, hwnd);
            request_redraw(hwnd);
            0
        }
        None => {
            // Unbound — fall through. WM_SYSKEYDOWN must reach DefWindowProc
            // so OS-level Alt+Space / Alt+F4 keep working.
            // SAFETY: defaulting unhandled keystrokes is canonical.
            unsafe { DefWindowProcW(hwnd, msg, vk as WPARAM, lparam) }
        }
    }
}

pub(super) fn handle_stack_tray_keydown(root: &AppRoot, vk: u32, hwnd: HWND) -> Option<LRESULT> {
    let (anchor, selected, members) = current_stack_tray_members(root)?;
    match vk {
        VK_UP_KEY | VK_DOWN_KEY => {
            let next = next_stack_tray_member(selected, &members, vk == VK_DOWN_KEY);
            root.dispatcher
                .push(Command::PreviewStackMember(anchor, next));
            request_redraw(hwnd);
            Some(0)
        }
        VK_ENTER => {
            root.dispatcher
                .push(Command::PreviewStackMember(anchor, selected));
            request_redraw(hwnd);
            Some(0)
        }
        VK_D_KEY | VK_DELETE_KEY => {
            root.dispatcher
                .push(Command::DetachStackMember(anchor, selected));
            request_redraw(hwnd);
            Some(0)
        }
        VK_U_KEY => {
            root.dispatcher.push(Command::DissolveStack(anchor));
            request_redraw(hwnd);
            Some(0)
        }
        VK_ESCAPE_KEY => {
            root.dispatcher.push(Command::CloseStackTray);
            request_redraw(hwnd);
            Some(0)
        }
        _ => None,
    }
}

pub(super) fn current_stack_tray_members(root: &AppRoot) -> Option<(ZoneId, ZoneId, Vec<ZoneId>)> {
    let app = root.app.borrow();
    if app.settings_open.get() || app.about_open.get() {
        return None;
    }
    let state = app.stack_tray.borrow().clone()?;
    let members = app.zones.stack_member_ids(state.anchor_zone_id)?.into_vec();
    let selected = if members.contains(&state.selected_member_id) {
        state.selected_member_id
    } else {
        members[0]
    };
    Some((state.anchor_zone_id, selected, members))
}

pub(super) fn next_stack_tray_member(
    selected: ZoneId,
    members: &[ZoneId],
    forward: bool,
) -> ZoneId {
    if members.is_empty() {
        return selected;
    }
    let index = members
        .iter()
        .position(|id| *id == selected)
        .unwrap_or_default();
    let next = if forward {
        (index + 1) % members.len()
    } else if index == 0 {
        members.len() - 1
    } else {
        index - 1
    };
    members[next]
}
