//! Native shell owner: `tray_minibars`.

use super::*;

// -----------------------------------------------------------------------------
// Tray icon registration (Ruling B)
// -----------------------------------------------------------------------------

pub(super) unsafe fn register_tray_icon(root: &AppRoot, hwnd: HWND) {
    if root.tray_registered.get() {
        return;
    }

    let icon = bentodesk_platform::window::load_tray_icon();
    if icon.is_null() {
        log_tray_error("LoadIconW(BentoDesk tray resource)", unsafe {
            GetLastError()
        });
        schedule_tray_retry(root, hwnd);
        return;
    }

    let uid_only = root.tray_uid_only.get();
    let delete_nid = build_tray_delete_icon_data(hwnd, uid_only);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &delete_nid);
    }

    let mut nid = build_tray_notify_icon_data(hwnd, icon, uid_only);
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
    if ok == 0 {
        log_tray_error("NIM_ADD", unsafe { GetLastError() });
        schedule_tray_retry(root, hwnd);
        return;
    }

    nid.Anonymous.uVersion = TRAY_ICON_VERSION;
    let version_ok = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &nid) };
    if version_ok == 0 {
        log_tray_error("NIM_SETVERSION", unsafe { GetLastError() });
        log_static("tray: NIM_ADD registered; NIM_SETVERSION degraded\n");
    } else {
        log_static("tray: NIM_ADD registered; NIM_SETVERSION=4\n");
    }

    unsafe {
        KillTimer(hwnd, TRAY_ICON_RETRY_TIMER_ID);
    }
    root.tray_registered.set(true);
    root.tray_retry_attempts.set(0);
}

pub(super) unsafe fn unregister_tray_icon(root: &AppRoot, hwnd: HWND) {
    unsafe {
        KillTimer(hwnd, TRAY_ICON_RETRY_TIMER_ID);
    }
    root.tray_registered.set(false);
    root.tray_retry_attempts.set(TRAY_ICON_MAX_RETRIES);

    let nid = build_tray_delete_icon_data(hwnd, root.tray_uid_only.get());
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub(super) fn build_tray_notify_icon_data(
    hwnd: HWND,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    uid_only: bool,
) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { core::mem::zeroed() };
    nid.cbSize = core::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    nid.uCallbackMessage = WM_TRAY_ICON;
    nid.hIcon = icon;
    nid.szTip = widen_static::<128>("BentoDesk");
    // Mc-3 #15 — the GUID identity is path-bound; the uID-only fallback drops
    // NIF_GUID + guidItem so the (hWnd, uID) identity registers on relocated
    // portable installs. `guidItem` stays zeroed from the `zeroed()` init.
    if uid_only {
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_SHOWTIP | NIF_TIP;
    } else {
        nid.uFlags = NIF_GUID | NIF_ICON | NIF_MESSAGE | NIF_SHOWTIP | NIF_TIP;
        nid.guidItem = TRAY_ICON_GUID;
    }
    nid
}

pub(super) fn build_tray_delete_icon_data(hwnd: HWND, uid_only: bool) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { core::mem::zeroed() };
    nid.cbSize = core::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    // Mc-3 #15 — delete the identity we actually registered. uID-only delete is
    // `uFlags = 0` with `guidItem` left zeroed; the GUID delete keeps NIF_GUID.
    if uid_only {
        nid.uFlags = 0;
    } else {
        nid.uFlags = NIF_GUID;
        nid.guidItem = TRAY_ICON_GUID;
    }
    nid
}

pub(super) fn tray_command_for_callback(_wparam: WPARAM, lparam: LPARAM) -> Option<Command> {
    match (lparam as u32) & 0xFFFF {
        WM_LBUTTONUP | NIN_SELECT => Some(Command::ShowWindow(WindowKind::Main)),
        WM_RBUTTONUP | WM_CONTEXTMENU => Some(Command::ShowTrayMenu),
        _ => None,
    }
}

pub(super) fn schedule_tray_retry(root: &AppRoot, hwnd: HWND) {
    let attempt = root.tray_retry_attempts.get();
    if attempt >= TRAY_ICON_MAX_RETRIES {
        // Mc-3 #15 — the GUID retry budget is spent. On a relocated portable
        // install the GUID is path-bound to the old EXE location and retrying
        // can never succeed, so fall back ONCE to a uID-only identity (no
        // NIF_GUID) and start a fresh budget. The flag is sticky, so this
        // GUID→uID transition happens at most once; the uID path then has its
        // own single budget that terminates below.
        if !root.tray_uid_only.get() {
            log_static(
                "tray: GUID NIM_ADD failed after budget; falling back to uID-only identity\n",
            );
            root.tray_uid_only.set(true);
            root.tray_retry_attempts.set(0);
            let timer =
                unsafe { SetTimer(hwnd, TRAY_ICON_RETRY_TIMER_ID, TRAY_ICON_RETRY_MS, None) };
            if timer == 0 {
                log_tray_error("SetTimer(TRAY_ICON_RETRY)", unsafe { GetLastError() });
            }
            return;
        }
        log_static("tray: NIM_ADD retry budget exhausted; continuing without tray icon\n");
        return;
    }
    root.tray_retry_attempts.set(attempt + 1);
    let timer = unsafe { SetTimer(hwnd, TRAY_ICON_RETRY_TIMER_ID, TRAY_ICON_RETRY_MS, None) };
    if timer == 0 {
        log_tray_error("SetTimer(TRAY_ICON_RETRY)", unsafe { GetLastError() });
    }
}

pub(super) fn log_tray_error(action: &str, error: u32) {
    tracing::warn!(
        target: "bentodesk::tray",
        action,
        error,
        "system tray operation failed"
    );
    let mut stderr = std::io::stderr();
    let _ = std::io::Write::write_fmt(
        &mut stderr,
        format_args!("tray: {action} failed (GetLastError={error})\n"),
    );
}

// -----------------------------------------------------------------------------
// Mouse handlers (T-010 — split: AppRoot for shared state, WindowSlot for per-HWND)
// -----------------------------------------------------------------------------

pub(super) fn minibar_command_for_pointer(
    app: &AppState,
    viewport: bentodesk_style::Size,
    x: f32,
    y: f32,
) -> Option<Command> {
    let (zone_id, bar) = app.active_minibar()?;
    let item_count = app
        .zones
        .get(zone_id)
        .map(|zone| zone.items.len())
        .unwrap_or(0);
    match minibar::minibar_hit_test_with_items(viewport, &bar, item_count, x, y) {
        Some(minibar::MiniBarHit::Unpin) => Some(Command::UnpinMinibar(zone_id)),
        Some(minibar::MiniBarHit::Item(index)) => app
            .zones
            .get(zone_id)
            .and_then(|zone| zone.items.get(index))
            .map(|item| Command::OpenItemFile(zone_id, bentodesk_app::ItemId(item.id.0))),
        _ => None,
    }
}

pub(super) fn minibar_tooltip_text_for_hover(
    app: &AppState,
    viewport: bentodesk_style::Size,
    x: f32,
    y: f32,
) -> Option<SmolStr> {
    let (zone_id, bar) = app.active_minibar()?;
    let item_count = app
        .zones
        .get(zone_id)
        .map(|zone| zone.items.len())
        .unwrap_or(0);
    match minibar::minibar_hit_test_with_items(viewport, &bar, item_count, x, y)? {
        minibar::MiniBarHit::Unpin => Some(localized_current(
            format!("取消固定迷你栏：{}", bar.label),
            format!("Unpin minibar {}", bar.label),
        )),
        minibar::MiniBarHit::Item(index) => {
            let item = app.zones.get(zone_id)?.items.get(index)?;
            let display_path = item_file_display_path(item);
            Some(localized_current(
                format!("打开 {display_path}"),
                format!("Open {display_path}"),
            ))
        }
        minibar::MiniBarHit::Body => Some(localized_current(
            format!("已固定区域：{}", bar.label),
            format!("Pinned zone {}", bar.label),
        )),
    }
}

pub(super) fn handle_minibar_lbutton_up(root: &AppRoot, slot: &WindowSlot, x: f32, y: f32) -> bool {
    if slot.kind != WindowKind::MiniBar {
        return false;
    }
    let viewport = window_slot_logical_viewport(slot);
    let command = {
        let app = root.app.borrow();
        minibar_command_for_pointer(&app, viewport, x, y)
    };
    match command {
        Some(command) => {
            root.dispatcher.push(command);
            true
        }
        None => false,
    }
}

pub(super) fn pin_zone_minibar_state(
    root: &AppRoot,
    zone_id: ZoneId,
) -> Option<MinibarPinStateChange> {
    let label = {
        let app = root.app.borrow();
        match app.zones.get(zone_id) {
            Some(zone) => SmolStr::new(zone.display_title()),
            None => {
                tracing::warn!(
                    target: "bentodesk::dispatcher",
                    ?zone_id,
                    "PinZoneAsMinibar refused: zone not found"
                );
                return None;
            }
        }
    };
    let pin_outcome = {
        let mut roster = root.minibar_roster.borrow_mut();
        if roster.contains(zone_id.0) {
            Ok(MinibarPinStateChange::Refreshed)
        } else {
            roster
                .pin(zone_id.0)
                .map(|_remaining| MinibarPinStateChange::Inserted)
        }
    };
    match pin_outcome {
        Ok(change) => {
            let event_base = u32::try_from(zone_id.0).unwrap_or(u32::MAX.saturating_sub(1));
            let unpin_event_id = event_base.saturating_add(1);
            let bar = MiniBar::new(ui::HIDE_PATH, label, unpin_event_id);
            root.app.borrow().upsert_minibar(zone_id, bar.clone());
            let mut bars = root.minibars.borrow_mut();
            if let Some(idx) = bars.iter().position(|(z, _)| *z == zone_id) {
                bars[idx] = (zone_id, bar);
            } else {
                bars.push((zone_id, bar));
            }
            Some(change)
        }
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::dispatcher",
                ?zone_id, error = %e,
                "PinZoneAsMinibar — business::minibar roster refused"
            );
            None
        }
    }
}

pub(super) fn rollback_zone_minibar_state(root: &AppRoot, zone_id: ZoneId) {
    let _ = root.minibar_roster.borrow_mut().unpin(zone_id.0);
    root.minibars.borrow_mut().retain(|(z, _)| *z != zone_id);
    let _ = root.app.borrow().remove_minibar(zone_id);
}

pub(super) fn pin_zone_as_minibar(root: &AppRoot, zone_id: ZoneId) -> bool {
    let Some(change) = pin_zone_minibar_state(root, zone_id) else {
        return false;
    };
    match ensure_aux_window(root, WindowKind::MiniBar) {
        Some(target) => {
            // SAFETY: ShowWindow canonical for any owned HWND.
            unsafe { ShowWindow(target, SW_SHOW) };
            request_redraw(target);
            tracing::info!(
                target: "bentodesk::dispatcher",
                ?zone_id,
                "PinZoneAsMinibar — business::minibar HWND shown"
            );
            log_static(format!("minibar: PinZoneAsMinibar shown zone_id={}\n", zone_id.0).as_str());
            true
        }
        None => {
            tracing::warn!(
                target: "bentodesk::dispatcher",
                ?zone_id,
                "PinZoneAsMinibar — ensure_aux_window failed; rolling back roster"
            );
            if change == MinibarPinStateChange::Inserted {
                rollback_zone_minibar_state(root, zone_id);
            }
            false
        }
    }
}

pub(super) fn unpin_zone_minibar(root: &AppRoot, zone_id: ZoneId) -> bool {
    let unpin_outcome = root.minibar_roster.borrow_mut().unpin(zone_id.0);
    match unpin_outcome {
        Ok(()) => {
            root.minibars.borrow_mut().retain(|(z, _)| *z != zone_id);
            let _ = root.app.borrow().remove_minibar(zone_id);
            if let Some(target) = find_aux_window(root, WindowKind::MiniBar) {
                if root.app.borrow().active_minibar().is_some() {
                    request_redraw(target);
                } else {
                    // SAFETY: ShowWindow canonical for any owned HWND.
                    unsafe { ShowWindow(target, SW_HIDE) };
                }
            }
            tracing::info!(
                target: "bentodesk::dispatcher",
                ?zone_id,
                "UnpinMinibar — business::minibar HWND hidden"
            );
            log_static(format!("minibar: UnpinMinibar hidden zone_id={}\n", zone_id.0).as_str());
            true
        }
        Err(e) => {
            tracing::debug!(
                target: "bentodesk::dispatcher",
                ?zone_id, error = %e,
                "UnpinMinibar — business::minibar roster reported no-op"
            );
            false
        }
    }
}
