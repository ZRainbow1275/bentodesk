//! Native shell owner: `context_menu_surface`.

use super::*;

pub(super) fn show_active_context_menu(
    root: &AppRoot,
    hwnd: HWND,
    cursor_x: f32,
    cursor_y: f32,
) -> Option<HWND> {
    {
        let app = root.app.borrow();
        let viewport = app.viewport;
        let mut active = app.active_context_menu.borrow_mut();
        let session = active.as_mut()?;
        session.submenu_open = false;
        session.hovered = None;
        session.submenu_scroll = 0;
        let closed = popover::context_menu_window_size(session);
        let expanded_width = if session.submenu_rows.is_empty() {
            closed.width
        } else {
            closed.width + popover::CONTEXT_MENU_SUBMENU_GAP + popover::CONTEXT_MENU_WIDTH
        };
        let submenu_on_left = !session.submenu_rows.is_empty()
            && cursor_x + expanded_width > viewport.width
            && cursor_x - expanded_width >= 0.0;
        session.submenu_on_left = submenu_on_left;
        let mut x = if submenu_on_left {
            cursor_x - closed.width
        } else {
            cursor_x
        };
        if expanded_width > viewport.width {
            x = 0.0;
        }
        x = x.clamp(0.0, (viewport.width - closed.width).max(0.0));
        let y = cursor_y.clamp(0.0, (viewport.height - closed.height).max(0.0));
        session.set_origin(x, y);
    }
    unsafe {
        for key in [
            0x01,
            0x02,
            VK_ESCAPE_KEY as i32,
            VK_LEFT_KEY as i32,
            VK_RIGHT_KEY as i32,
            VK_UP_KEY as i32,
            VK_DOWN_KEY as i32,
            VK_ENTER as i32,
        ] {
            let _ = GetAsyncKeyState(key);
        }
        SetTimer(
            hwnd,
            CONTEXT_MENU_INPUT_TIMER_ID,
            CONTEXT_MENU_INPUT_POLL_MS,
            None,
        );
        // Native popup semantics without native popup chrome: capture keeps
        // fast clicks outside the clipped Main window region routable to the
        // menu's existing pointer handlers. GetAsyncKeyState's low bit alone
        // is process-global and can be consumed by another thread.
        SetCapture(hwnd);
    }
    request_redraw(hwnd);
    Some(hwnd)
}

pub(super) fn poll_context_menu_input(root: &AppRoot, hwnd: HWND) {
    if root.app.borrow().active_context_menu.borrow().is_none() {
        unsafe { KillTimer(hwnd, CONTEXT_MENU_INPUT_TIMER_ID) };
        return;
    }
    // SetCapture called from WM_RBUTTONUP can be released when that native
    // button sequence unwinds. Reassert it from the later timer tick so fast
    // clicks outside Main's clipped region are delivered to this HWND.
    unsafe { SetCapture(hwnd) };

    for vk in [
        VK_ESCAPE_KEY,
        VK_LEFT_KEY,
        VK_RIGHT_KEY,
        VK_UP_KEY,
        VK_DOWN_KEY,
        VK_ENTER,
    ] {
        if unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x0001 != 0 } {
            handle_context_menu_keydown(root, hwnd, vk);
            return;
        }
    }

    let clicked = unsafe {
        async_mouse_button_active(GetAsyncKeyState(0x01))
            || async_mouse_button_active(GetAsyncKeyState(0x02))
    };
    if !clicked {
        return;
    }
    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 || unsafe { ScreenToClient(hwnd, &mut point) } == 0
    {
        return;
    }
    let dpi = bento_nano_platform::dpi::get_dpi_for_window(hwnd).max(96);
    let x = bento_nano_style::dpi::device_to_logical_f32(point.x as f32, dpi);
    let y = bento_nano_style::dpi::device_to_logical_f32(point.y as f32, dpi);
    let inside = root
        .app
        .borrow()
        .active_context_menu
        .borrow()
        .as_ref()
        .is_some_and(|session| popover::context_menu_contains(session, x, y));
    if !inside {
        close_context_menu_surface(root);
    }
}

#[inline]
pub(super) fn async_mouse_button_active(state: i16) -> bool {
    // High bit is the current physical-down state and cannot be consumed by
    // another caller; low bit preserves sub-tick clicks observed since the
    // previous query.
    state as u16 & 0x8001 != 0
}

pub(super) fn context_menu_row_for_hit(
    session: &popover::ContextMenuSession,
    hit: popover::ContextMenuHit,
) -> Option<&popover::ContextMenuRow> {
    match hit.column {
        popover::ContextMenuColumn::Main => session.main_rows.get(hit.row),
        popover::ContextMenuColumn::Submenu => session.submenu_rows.get(hit.row),
    }
}

pub(super) fn context_menu_next_hit(
    session: &popover::ContextMenuSession,
    column: popover::ContextMenuColumn,
    current: Option<usize>,
    forward: bool,
) -> Option<popover::ContextMenuHit> {
    let (start, end) = match column {
        popover::ContextMenuColumn::Main => (0, session.main_rows.len()),
        popover::ContextMenuColumn::Submenu => {
            let range = session.visible_submenu_range();
            (range.start, range.end)
        }
    };
    let len = end.saturating_sub(start);
    if len == 0 {
        return None;
    }
    let base = current.filter(|row| *row >= start && *row < end);
    for step in 1..=len {
        let offset = match (base, forward) {
            (Some(row), true) => (row - start + step) % len,
            (Some(row), false) => (row - start + len - (step % len)) % len,
            (None, true) => step - 1,
            (None, false) => len - step,
        };
        let row = start + offset;
        let candidate = popover::ContextMenuHit { column, row };
        if context_menu_row_for_hit(session, candidate)
            .is_some_and(|entry| entry.kind != popover::ContextMenuRowKind::Separator)
        {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn activate_context_menu_hit(root: &AppRoot, hwnd: HWND, hit: popover::ContextMenuHit) {
    let (kind, command_id) = {
        let app = root.app.borrow();
        let active = app.active_context_menu.borrow();
        let Some(session) = active.as_ref() else {
            return;
        };
        let Some(row) = context_menu_row_for_hit(session, hit) else {
            return;
        };
        (row.kind, row.command_id)
    };
    if kind == popover::ContextMenuRowKind::Submenu {
        resize_context_menu_for_submenu(root, hwnd, true);
        return;
    }
    if kind != popover::ContextMenuRowKind::Command {
        return;
    }

    let zone_pending = root.zone_context_menu.borrow().clone();
    let item_pending = root.item_context_menu.borrow().clone();
    let zone_action = zone_pending
        .as_ref()
        .and_then(|pending| zone_context_action_for_choice(command_id, &pending.stack_targets));
    let item_action = item_pending.as_ref().and_then(|pending| {
        item_context_action_for_choice_with(command_id, |target_command_id| {
            pending
                .move_targets
                .iter()
                .find(|(candidate, _)| *candidate == target_command_id)
                .map(|(_, zone_id)| *zone_id)
        })
    });
    close_context_menu_surface(root);

    if let (Some(pending), Some(action)) = (zone_pending, zone_action) {
        log_static(
            format!(
                "zone_menu: chosen={} zone_id={} style=d2d submenu_targets={}\n",
                command_id,
                pending.zone_id.0,
                pending.stack_targets.len()
            )
            .as_str(),
        );
        apply_zone_context_action(root, pending.zone_id, action);
    } else if let (Some(pending), Some(action)) = (item_pending, item_action) {
        log_static(
            format!(
                "item_menu: chosen={} zone_id={} item_id={} style=d2d submenu_targets={}\n",
                command_id,
                pending.zone_id.0,
                pending.item_id.0,
                pending.move_targets.len()
            )
            .as_str(),
        );
        apply_item_context_dispatch(
            root,
            item_context_dispatch_for_action(
                pending.zone_id,
                pending.item_id,
                pending.path.as_str(),
                action,
            ),
        );
    }
    let redraw = find_main_hwnd(root).unwrap_or(hwnd);
    consume_dispatcher(root, redraw);
    request_redraw(redraw);
}

pub(super) fn handle_context_menu_mouse_move(root: &AppRoot, hwnd: HWND, x: f32, y: f32) {
    let (changed, open_submenu) = {
        let app = root.app.borrow();
        let mut active = app.active_context_menu.borrow_mut();
        let Some(session) = active.as_mut() else {
            return;
        };
        let hit = popover::context_menu_hit_test(session, x, y);
        let changed = session.hovered != hit;
        session.hovered = hit;
        let open_submenu = hit
            .and_then(|candidate| context_menu_row_for_hit(session, candidate))
            .is_some_and(|row| row.kind == popover::ContextMenuRowKind::Submenu);
        (changed, open_submenu)
    };
    if open_submenu {
        resize_context_menu_for_submenu(root, hwnd, true);
    }
    if changed {
        request_redraw(hwnd);
    }
}

pub(super) fn handle_context_menu_lbutton_up(root: &AppRoot, hwnd: HWND, x: f32, y: f32) {
    let hit = {
        let app = root.app.borrow();
        let active = app.active_context_menu.borrow();
        active
            .as_ref()
            .and_then(|session| popover::context_menu_hit_test(session, x, y))
    };
    if let Some(hit) = hit {
        activate_context_menu_hit(root, hwnd, hit);
    } else {
        close_context_menu_surface(root);
    }
}

pub(super) fn handle_context_menu_mousewheel(root: &AppRoot, hwnd: HWND, wparam: WPARAM) -> bool {
    let Some(delta) = settings_wheel_scroll_delta_from_wparam(wparam) else {
        return false;
    };
    let changed = {
        let app = root.app.borrow();
        let mut active = app.active_context_menu.borrow_mut();
        let Some(session) = active.as_mut() else {
            return false;
        };
        if !session.submenu_open
            || session.submenu_rows.len() <= popover::CONTEXT_MENU_MAX_SUBMENU_ROWS
        {
            return true;
        }
        let before = session.submenu_scroll;
        if delta > 0 {
            session.submenu_scroll = session.submenu_scroll.saturating_add(1);
        } else {
            session.submenu_scroll = session.submenu_scroll.saturating_sub(1);
        }
        session.clamp_submenu_scroll();
        before != session.submenu_scroll
    };
    if changed {
        request_redraw(hwnd);
    }
    true
}

pub(super) fn handle_context_menu_keydown(root: &AppRoot, hwnd: HWND, vk: u32) -> LRESULT {
    if vk == VK_ESCAPE_KEY {
        close_context_menu_surface(root);
        return 0;
    }
    if vk == VK_LEFT_KEY {
        resize_context_menu_for_submenu(root, hwnd, false);
        return 0;
    }

    let mut activate = None;
    let mut open_submenu = false;
    let changed = {
        let app = root.app.borrow();
        let mut active = app.active_context_menu.borrow_mut();
        let Some(session) = active.as_mut() else {
            return 0;
        };
        match vk {
            VK_UP_KEY | VK_DOWN_KEY => {
                let column = session
                    .hovered
                    .map(|hit| hit.column)
                    .unwrap_or(popover::ContextMenuColumn::Main);
                let next = context_menu_next_hit(
                    session,
                    column,
                    session.hovered.map(|hit| hit.row),
                    vk == VK_DOWN_KEY,
                );
                let changed = session.hovered != next;
                session.hovered = next;
                changed
            }
            VK_RIGHT_KEY => {
                open_submenu = session
                    .hovered
                    .and_then(|hit| context_menu_row_for_hit(session, hit))
                    .is_some_and(|row| row.kind == popover::ContextMenuRowKind::Submenu);
                false
            }
            VK_ENTER => {
                activate = session.hovered;
                false
            }
            _ => false,
        }
    };
    if open_submenu {
        resize_context_menu_for_submenu(root, hwnd, true);
        let app = root.app.borrow();
        let mut active = app.active_context_menu.borrow_mut();
        if let Some(session) = active.as_mut() {
            session.hovered =
                context_menu_next_hit(session, popover::ContextMenuColumn::Submenu, None, true);
        }
        request_redraw(hwnd);
    } else if let Some(hit) = activate {
        activate_context_menu_hit(root, hwnd, hit);
    } else if changed {
        request_redraw(hwnd);
    }
    0
}

pub(super) fn show_zone_context_menu(root: &AppRoot, hwnd: HWND, x: f32, y: f32, zone_id: ZoneId) {
    type ZoneStackMenuTargets = Vec<(usize, ZoneId, SmolStr)>;
    let (live_folder_bound, stack_tray_available, stack_targets): (
        bool,
        bool,
        ZoneStackMenuTargets,
    ) = {
        let app = root.app.borrow();
        let live_folder_bound = app
            .zones
            .get(zone_id)
            .and_then(|zone| zone.live_folder_path.as_ref())
            .is_some();
        let stack_tray_available = app.zones.stack_member_ids_for(zone_id).is_some();
        let stack_targets = app
            .zones
            .iter()
            .filter(|zone| zone.id != zone_id && !zone.is_stacked_child())
            .take(64)
            .enumerate()
            .map(|(idx, zone)| {
                (
                    ZONE_CONTEXT_STACK_BASE_ID + idx,
                    zone.id,
                    SmolStr::new(zone.display_title()),
                )
            })
            .collect();
        (live_folder_bound, stack_tray_available, stack_targets)
    };

    let top_entries = zone_context_menu_rows(
        live_folder_bound,
        stack_tray_available,
        !stack_targets.is_empty(),
    );
    let mut submenu_entries = popover::ContextMenuRows::new();
    for (command_id, _, label) in &stack_targets {
        submenu_entries.push(popover::ContextMenuRow::command(
            *command_id,
            label.as_str(),
            IconKind::Grid,
        ));
    }
    let stack_command_targets: Vec<(usize, ZoneId)> = stack_targets
        .iter()
        .map(|(command_id, target_zone_id, _)| (*command_id, *target_zone_id))
        .collect();
    root.zone_context_menu
        .borrow_mut()
        .replace(PendingZoneContextMenu {
            zone_id,
            stack_targets: stack_command_targets.clone(),
        });
    root.item_context_menu.borrow_mut().take();
    root.app
        .borrow()
        .active_context_menu
        .borrow_mut()
        .replace(popover::ContextMenuSession::new(
            top_entries,
            submenu_entries,
        ));
    let shown = show_active_context_menu(root, hwnd, x, y).is_some();
    log_static(
        format!(
            "zone_menu: opened={} zone_id={} style=d2d submenu_targets={}\n",
            shown,
            zone_id.0,
            stack_command_targets.len()
        )
        .as_str(),
    );
    if !shown {
        close_context_menu_surface(root);
    }
}

pub(super) fn show_item_context_menu(
    root: &AppRoot,
    hwnd: HWND,
    x: f32,
    y: f32,
    zone_id: ZoneId,
    item_id: bento_nano_zone::ZoneItemId,
    path: &str,
) {
    let move_targets: Vec<(usize, ZoneId, SmolStr)> = {
        let app = root.app.borrow();
        app.zones
            .iter()
            .filter(|zone| zone.id != zone_id)
            .take(64)
            .enumerate()
            .map(|(idx, zone)| {
                (
                    ITEM_CONTEXT_MOVE_ZONE_BASE_ID + idx,
                    zone.id,
                    SmolStr::new(zone.display_title()),
                )
            })
            .collect()
    };
    let top_entries = item_context_menu_rows(!move_targets.is_empty());
    let mut submenu_entries = popover::ContextMenuRows::new();
    for (command_id, _, label) in &move_targets {
        submenu_entries.push(popover::ContextMenuRow::command(
            *command_id,
            label.as_str(),
            IconKind::Grid,
        ));
    }
    let move_command_targets: Vec<(usize, ZoneId)> = move_targets
        .iter()
        .map(|(command_id, target_zone_id, _)| (*command_id, *target_zone_id))
        .collect();
    root.item_context_menu
        .borrow_mut()
        .replace(PendingItemContextMenu {
            zone_id,
            item_id,
            path: SmolStr::new(path),
            move_targets: move_command_targets.clone(),
        });
    root.zone_context_menu.borrow_mut().take();
    root.app
        .borrow()
        .active_context_menu
        .borrow_mut()
        .replace(popover::ContextMenuSession::new(
            top_entries,
            submenu_entries,
        ));
    let shown = show_active_context_menu(root, hwnd, x, y).is_some();
    log_static(
        format!(
            "item_menu: opened={} zone_id={} item_id={} style=d2d submenu_targets={}\n",
            shown,
            zone_id.0,
            item_id.0,
            move_command_targets.len()
        )
        .as_str(),
    );
    if !shown {
        close_context_menu_surface(root);
    }
}

pub(super) fn copy_text_to_clipboard(owner: HWND, text: &str) -> bool {
    let wide = widen_dynamic(text);
    let byte_len = wide.len() * core::mem::size_of::<u16>();
    unsafe {
        if OpenClipboard(owner) == 0 {
            return false;
        }
        let success = copy_text_to_open_clipboard(&wide, byte_len);
        let _ = CloseClipboard();
        success
    }
}

pub(super) fn copy_text_to_open_clipboard(wide: &[u16], byte_len: usize) -> bool {
    unsafe {
        if EmptyClipboard() == 0 {
            return false;
        }
        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if hmem.is_null() {
            return false;
        }
        let locked = GlobalLock(hmem);
        if locked.is_null() {
            let _ = GlobalFree(hmem);
            return false;
        }
        ptr::copy_nonoverlapping(wide.as_ptr(), locked.cast::<u16>(), wide.len());
        let _ = GlobalUnlock(hmem);
        if SetClipboardData(CF_UNICODETEXT_ID, hmem).is_null() {
            let _ = GlobalFree(hmem);
            return false;
        }
        true
    }
}

pub(super) fn shell_execute_path(
    verb: &str,
    file: &str,
    parameters: Option<&str>,
) -> Result<(), i32> {
    let verb_w = wide_z(verb);
    let file_w = wide_z(file);
    let params_w = parameters.map(wide_z);
    let params_ptr = params_w
        .as_ref()
        .map(|v| v.as_ptr())
        .unwrap_or(core::ptr::null());
    let result = unsafe {
        ShellExecuteW(
            core::ptr::null_mut(),
            verb_w.as_ptr(),
            file_w.as_ptr(),
            params_ptr,
            core::ptr::null(),
            SW_SHOW,
        )
    };
    if (result as isize) <= 32 {
        let code = result as i32;
        tracing::warn!(
            target: "bentodesk::items",
            verb,
            file,
            code,
            "ShellExecuteW failed"
        );
        return Err(code);
    }
    Ok(())
}

pub(super) fn reveal_path_in_explorer(path: &str) -> Result<(), i32> {
    let params = format!("/select,\"{path}\"");
    shell_execute_path("open", "explorer.exe", Some(&params))
}

pub(super) fn wide_z(value: &str) -> Vec<u16> {
    let mut out: Vec<u16> = value.encode_utf16().collect();
    out.push(0);
    out
}

pub(super) fn push_button_command(root: &AppRoot, event_id: u32) {
    if event_id == ui::events::PIN {
        root.dispatcher.push(Command::TogglePin);
    } else if event_id == ui::events::SETTINGS {
        root.dispatcher.push(Command::ToggleSettings);
    } else if event_id == ui::events::HIDE {
        root.dispatcher.push(Command::HideWindow(WindowKind::Main));
    } else if event_id == ui::events::ADD_ZONE {
        root.dispatcher
            .push(Command::CreateZone(default_zone_spec(root)));
    } else if event_id == ui::events::EXIT {
        root.dispatcher.push(Command::QuitApp);
    }
}

/// Build a default `ZoneSpec` for the in-shell "+" button + `Ctrl+N` hotkey
/// path. T-013 widened `Command::CreateZone` to carry a spec; the shell's
/// two legacy producers compose one centred at the current viewport.
pub(super) fn default_zone_spec(root: &AppRoot) -> ZoneSpec {
    let app = root.app.borrow();
    let cx = app.viewport.width * 0.5;
    let cy = app.viewport.height * 0.5;
    ZoneSpec {
        name: smol_str::SmolStr::new_static("Zone"),
        origin: DispatchPoint::new((cx as i32) - 100, (cy as i32) - 60),
        size: DispatchSize::new(200, 120),
    }
}
