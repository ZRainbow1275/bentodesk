//! Native shell owner: `main_window_proc`.

use super::*;

// -----------------------------------------------------------------------------
// Window procedure
// -----------------------------------------------------------------------------

pub(super) unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Mc-1b — TaskbarCreated. Explorer broadcasts this registered message to
    // every top-level window when its tray is (re)created — notably after an
    // explorer.exe crash/restart, which otherwise drops our notify icon for
    // the rest of the session. The message id is dynamic (not a compile-time
    // constant), so it is matched here with an early `if` rather than a match
    // arm. Lazily register the id once via OnceLock.
    let taskbar_created = *TASKBAR_CREATED_MSG.get_or_init(|| {
        let mut name: Vec<u16> = "TaskbarCreated".encode_utf16().collect();
        name.push(0);
        // SAFETY: `name` is NUL-terminated; RegisterWindowMessageW only reads.
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    });
    if taskbar_created != 0 && msg == taskbar_created {
        // Re-add the tray icon. `register_tray_icon` early-returns when
        // `tray_registered` is already true, so we must clear that flag (and
        // the retry budget) first or the re-registration is a no-op. The
        // `&AppRoot` is obtained via the process-global `app_root()` — the
        // IDENTICAL acquisition the WM_TIMER tray-retry arm uses. A null root
        // (pre-install_app_root) just returns without crashing.
        if let Some(root) = app_root() {
            root.tray_registered.set(false);
            root.tray_retry_attempts.set(0);
            // Mc-3 #15 — deliberately do NOT reset `tray_uid_only`: if the GUID
            // identity was already proven unusable this session, stay uID-only.
            // SAFETY: `hwnd` is this window's live handle; register_tray_icon
            //         only touches the tray + the (single-threaded) AppRoot.
            unsafe {
                register_tray_icon(root, hwnd);
            }
        }
        return 0;
    }

    match msg {
        WM_NCCREATE => {
            // SAFETY: lparam is a valid CREATESTRUCTW pointer per WM_NCCREATE.
            let _cs = unsafe { &*(lparam as *const CREATESTRUCTW) };
            // SAFETY: DefWindowProc must be called for WM_NCCREATE per docs.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CREATE => {
            // Ruling B — register the tray icon as soon as the Main window
            // exists; actual Shell_NotifyIconW registration is timer-deferred.
            // WM_DESTROY removes it.
            if let Some(root) = app_root() {
                schedule_tray_retry(root, hwnd);
                // SAFETY: `hwnd` is the just-created Main HWND.
                unsafe { register_global_hotkeys(root, hwnd) };
                start_startup_icon_rehydrate(root, hwnd);
            }
            // SAFETY: `hwnd` is the just-created Main HWND. Timer messages
            // are delivered on the same UI thread and use no callback.
            unsafe {
                SetTimer(
                    hwnd,
                    BACKEND_EVENT_POLL_TIMER_ID,
                    BACKEND_EVENT_POLL_MS,
                    None,
                );
                let trim_timer = SetTimer(
                    hwnd,
                    STARTUP_MEMORY_TRIM_TIMER_ID,
                    STARTUP_MEMORY_TRIM_MS,
                    None,
                );
                if trim_timer == 0 {
                    log_static(
                        format!(
                            "memory: SetTimer(STARTUP_MEMORY_TRIM) failed (GetLastError={})\n",
                            GetLastError()
                        )
                        .as_str(),
                    );
                }
                arm_resident_memory_trim(hwnd);
            }
            0
        }
        WM_POWERBROADCAST => {
            if wparam == PBT_APMRESUMEAUTOMATIC {
                if let Some(root) = app_root() {
                    schedule_power_resume(root);
                }
                return 1;
            }
            // SAFETY: unhandled power notifications retain the default window
            // procedure semantics.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // Mc-1b — a second launch attempt posts this to the already-running
        // instance (single-instance guard). Surface the Main window. Harmless
        // no-op if it is already visible/foreground.
        x if x == WM_WAKE_INSTANCE => {
            // SAFETY: `hwnd` is this window's live handle.
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            }
            0
        }
        x if x == WM_ICON_CACHE_READY => {
            if let Some(root) = app_root() {
                consume_dispatcher(root, hwnd);
                request_redraw(hwnd);
            }
            0
        }
        WM_PAINT => {
            // SAFETY: paint handles its own state (lazy slot init).
            unsafe {
                match paint(hwnd) {
                    Ok(()) => {
                        // A successful frame clears the failure streak. Mc-2
                        // device-recovery will likewise reset this once the
                        // device is rebuilt, so a recoverable device-loss
                        // never reaches the fatal threshold below.
                        PAINT_FAIL_STREAK.store(0, Ordering::Relaxed);
                        // Mc-2b — a clean frame means the device is stable
                        // again, so drop the recovery state back to `Healthy`
                        // and clear the retry window. A later, unrelated device
                        // loss then starts with a fresh attempt budget.
                        if let Some(root) = app_root() {
                            if root.recovery_state.get()
                                != bentodesk_platform::RecoveryState::Healthy
                            {
                                root.recovery_state
                                    .set(bentodesk_platform::RecoveryState::Healthy);
                                root.last_recovery_at.set(None);
                            }
                        }
                    }
                    // Mc-2b — a lost device routes to recovery instead of the
                    // failure streak: `handle_device_lost` recreates the chain
                    // + this renderer (and resets PAINT_FAIL_STREAK on success)
                    // or escalates via the retry cap. Do NOT increment the
                    // streak here — recovery owns that decision.
                    Err(bentodesk_app::RenderError::DeviceLost) => {
                        if let Some(root) = app_root() {
                            handle_device_lost(root, hwnd);
                        }
                    }
                    Err(e) => {
                        log_paint_err(e);
                        // Mc-1b(c) — a permanently-dead renderer keeps failing
                        // every frame. After a deliberately high threshold
                        // (so transient device-loss never trips it) surface a
                        // single fatal box and quit cleanly rather than spin
                        // the WM_PAINT loop forever, invisibly, burning CPU.
                        let n = PAINT_FAIL_STREAK.fetch_add(1, Ordering::Relaxed) + 1;
                        if n >= PAINT_FATAL_STREAK_THRESHOLD
                            && !PAINT_FATAL_SHOWN.swap(true, Ordering::Relaxed)
                        {
                            show_fatal_box(
                                "BentoDesk — 渲染不可用 / Rendering unavailable",
                                "Direct3D/Direct2D renderer could not initialise on this system.",
                            );
                            PostQuitMessage(2);
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_SIZE => {
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let new_w = (lparam as u32) & 0xFFFF;
                    let new_h = ((lparam as u32) >> 16) & 0xFFFF;
                    let slot = &mut *p;
                    // Mc-2b / #10 — stop swallowing device-loss on resize. The
                    // `&mut slot` borrow ends at the `;`, so `handle_device_lost`
                    // (which re-fetches the slot) does not alias it. Other
                    // resize errors stay swallowed as before.
                    if let Err(bentodesk_app::RenderError::DeviceLost) =
                        slot.renderer.resize(new_w, new_h)
                    {
                        if let Some(root) = app_root() {
                            handle_device_lost(root, hwnd);
                        }
                    }
                }
            }
            0
        }
        WM_DPICHANGED => {
            // PER_MONITOR_AWARE_V2 contract. T-012: rebuild swap chain at
            // new monitor's pixel density (the OS suggested rect arrives in
            // the *new monitor's* device pixels, so the backbuffer must
            // follow or the next frame paints at the wrong resolution).
            // T-012 / R4 — DPI cache is per-HWND (`WindowSlot.state.dpi`),
            // never global.
            //
            // SAFETY: slot pointer fetched from window data — null-checked;
            //         lParam is non-null per WM_DPICHANGED ABI guarantee.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let new_dpi = (wparam as u32) & 0xFFFF;
                    let slot = &mut *p;
                    slot.state.dpi.set(new_dpi);
                    slot.state.monitors = bentodesk_platform::enumerate_monitors();

                    if !(lparam as *const RECT).is_null() {
                        // SAFETY: WM_DPICHANGED ABI guarantees lParam is a
                        //         valid pointer to a RECT for the duration
                        //         of the message dispatch.
                        let r = &*(lparam as *const RECT);
                        let new_w = (r.right - r.left).max(1) as u32;
                        let new_h = (r.bottom - r.top).max(1) as u32;
                        SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        // T-012 — rebuild swap chain at new monitor's pixel
                        // density. `Renderer::resize` re-passes the swap
                        // chain flags so the FRAME_LATENCY_WAITABLE_OBJECT
                        // doesn't get demoted (Wave 12 contract).
                        // Mc-2b / #10 — route a device loss on this resize into
                        // recovery instead of discarding it.
                        if let Err(bentodesk_app::RenderError::DeviceLost) =
                            slot.renderer.resize(new_w, new_h)
                        {
                            if let Some(root) = app_root() {
                                handle_device_lost(root, hwnd);
                            }
                        }
                    }
                }
            }
            0
        }
        WM_DISPLAYCHANGE => {
            // Display hotplug refresh (USB monitor unplugged, projector
            // connected, resolution change in Display Settings). WM_DPICHANGED
            // only fires when the per-window DPI changes; physical display
            // reconfiguration reaches us through WM_DISPLAYCHANGE instead.
            //
            // Wave C (05-20 visual parity) — Main HWND tracks the primary
            // monitor work area, so on every display reconfiguration we
            // re-snap to `main_window_rect()` and resize the swap chain to
            // the new device-pixel extent. Non-Main slots keep the existing
            // refresh-only behaviour (their geometry is owned by per-aux
            // window code).
            //
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let slot = &mut *p;
                    slot.state.monitors = bentodesk_platform::enumerate_monitors();
                    if slot.kind == WindowKind::Main {
                        let (x, y, w, h) = bentodesk_platform::main_window_rect();
                        SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            x,
                            y,
                            w.max(1),
                            h.max(1),
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        // Frosted-backdrop — the resolution / monitor topology
                        // changed, so the captured work-area snapshot is stale
                        // (wrong size / wrong wallpaper region). Mark it dirty so
                        // the next Main paint re-captures the new primary work
                        // area; cheap flag flip, the capture is deferred.
                        slot.renderer.mark_backdrop_dirty();
                        // Mc-2b / #10 — route a device loss on this resize into
                        // recovery instead of discarding it.
                        if let Err(bentodesk_app::RenderError::DeviceLost) =
                            slot.renderer.resize(w.max(1) as u32, h.max(1) as u32)
                        {
                            if let Some(root) = app_root() {
                                handle_device_lost(root, hwnd);
                            }
                        }
                    }
                }
            }
            0
        }
        WM_SETTINGCHANGE => {
            // Frosted-backdrop — the user changed a system setting; the desktop
            // wallpaper change arrives here as `SPI_SETDESKWALLPAPER`. We don't
            // bother decoding `wparam` (marking on ANY settingchange is cheap —
            // it only flips a flag; the actual re-capture is deferred to the
            // next Main paint), so any settingchange refreshes the captured
            // work-area snapshot for the Main overlay. Non-Main slots have no
            // backdrop, so this is a no-op for them.
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let slot = &mut *p;
                    if slot.kind == WindowKind::Main {
                        slot.renderer.mark_backdrop_dirty();
                    }
                }
            }
            // Pass through to the default proc so other settingchange handling
            // (e.g. system font / theme propagation) is not swallowed.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SHOWWINDOW => {
            // T-099 — hibernation entry point. Track visibility state per
            // slot; the slot's `set_visible` flips `pending_hibernate` for
            // non-Main windows on hide. Actual swap chain release happens
            // on the next paint pump cycle once `HIBERNATE_GATE_MS` (500 ms)
            // has elapsed — see `flush_hibernation`.
            //
            // wParam: TRUE = window is shown, FALSE = hidden.
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let visible = wparam != 0;
                    let slot = &*p;
                    let live_dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd);
                    if live_dpi != 0 {
                        slot.state.dpi.set(live_dpi);
                    }
                    slot.set_visible(visible, GetTickCount());
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEMOVE => {
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let dx = (lparam as i32 & 0xFFFF) as i16 as f32;
                    let dy = ((lparam as i32 >> 16) & 0xFFFF) as i16 as f32;
                    let slot = &*p;
                    if slot.kind == WindowKind::Main {
                        let mut tracking = TRACKMOUSEEVENT {
                            cbSize: core::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        // SAFETY: `hwnd` is the live Main HWND currently dispatching
                        // WM_MOUSEMOVE, and `tracking` points to initialized stack storage.
                        let _ = TrackMouseEvent(&mut tracking);
                    }
                    let live_dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd);
                    if live_dpi != 0 {
                        slot.state.dpi.set(live_dpi);
                    }
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        handle_mouse_move(root, slot, x, y);
                        apply_ghost_cursor_passthrough_for_point(root, slot, x, y);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        m if m == WM_ITEM_DRAG_OUT => {
            if let Some(root) = app_root() {
                if let Some(request) = root.pending_item_drag_out.borrow_mut().take() {
                    log_static(
                        format!(
                            "items: drag-out deferred-start zone={} item={} copy={} path={}\n",
                            request.zone_id.0, request.item_id.0, request.copy_only, request.path
                        )
                        .as_str(),
                    );
                    // SAFETY: the shell used capture only to detect the
                    // item-drag threshold. OLE owns mouse capture during
                    // `DoDragDrop`; keeping BentoDesk's capture here can
                    // prevent the OLE modal loop from observing pointer
                    // transitions and calling `IDropSource::QueryContinueDrag`.
                    unsafe { ReleaseCapture() };
                    start_item_drag_out(root, hwnd, request);
                    // SAFETY: defensive cleanup for failed/cancelled OLE
                    // paths. Normal OLE completion should already have
                    // released its own capture.
                    unsafe { ReleaseCapture() };
                    request_redraw(hwnd);
                }
            }
            0
        }
        WM_TIMER => handle_main_window_timer(hwnd, msg, wparam, lparam),
        WM_COMMAND => {
            if let Some(root) = app_root() {
                let command_id = (wparam as u32 & 0xFFFF) as usize;
                if handle_tray_wm_command(root, hwnd, command_id) {
                    return 0;
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSELEAVE => {
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    if let Some(root) = app_root() {
                        clear_hover(root);
                        arm_hover_frame_timer(hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_MOUSEWHEEL => unsafe {
            let p = get_slot_ptr(hwnd);
            if !p.is_null() {
                let slot = &*p;
                if let Some(root) = app_root() {
                    if root.app.borrow().active_context_menu.borrow().is_some()
                        && handle_context_menu_mousewheel(root, hwnd, wparam)
                    {
                        return 0;
                    }
                    if handle_settings_mousewheel(root, slot, hwnd, wparam) {
                        return 0;
                    }
                    if handle_zone_mousewheel(root, slot, hwnd, wparam) {
                        return 0;
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
        WM_LBUTTONDOWN => {
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let dx = (lparam as i32 & 0xFFFF) as i16 as f32;
                    let dy = ((lparam as i32 >> 16) & 0xFFFF) as i16 as f32;
                    let slot = &*p;
                    let live_dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd);
                    if live_dpi != 0 {
                        slot.state.dpi.set(live_dpi);
                    }
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        handle_lbutton_down(root, slot, hwnd, x, y);
                        // Pointer producers queue business commands. Reduce them
                        // before the next paint so the click cannot present one
                        // stale frame and wait for another mouse move to update.
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                        let geometry_drag_armed = {
                            let app = root.app.borrow();
                            app.zone_drag.get().is_some() || app.zone_resize.get().is_some()
                        };
                        if geometry_drag_armed {
                            // The renderer expands Main's exact chrome region to
                            // full-client while a move/resize owns capture. Commit
                            // that unchanged pointer-down frame synchronously so
                            // the first WM_MOUSEMOVE never combines SetWindowRgn
                            // with the first moved DComp present (a one-frame
                            // transparent flash on a cold drag).
                            UpdateWindow(hwnd);
                            // `SetWindowRgn` changes DWM's visible clip
                            // synchronously, while the DComp commit above is
                            // consumed asynchronously. Wait once at arm time so
                            // there is no compositor interval with the new clip
                            // but no matching surface. This is deliberately not
                            // part of the per-WM_MOUSEMOVE hot path.
                            let _ = DwmFlush();
                        }
                    }
                }
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let dx = (lparam as i32 & 0xFFFF) as i16 as f32;
                    let dy = ((lparam as i32 >> 16) & 0xFFFF) as i16 as f32;
                    let slot = &*p;
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        handle_lbutton_double_click(root, slot, hwnd, x, y);
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let dx = (lparam as i32 & 0xFFFF) as i16 as f32;
                    let dy = ((lparam as i32 >> 16) & 0xFFFF) as i16 as f32;
                    let slot = &*p;
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        handle_lbutton_up(root, slot, hwnd, x, y);
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_DROPFILES => {
            unsafe {
                log_static("wm_dropfiles: received\n");
                let hdrop = wparam as HDROP;
                let mut raw_payload = false;
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let slot = &*p;
                    if let Some(root) = app_root() {
                        raw_payload = handle_drop_files(root, slot, hdrop);
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
                finish_drop_files_handle(hdrop, raw_payload);
            }
            0
        }
        WM_RBUTTONUP => {
            // Right-click in the client area — only used for zone delete
            // today (Ruling D).
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let dx = (lparam as i32 & 0xFFFF) as i16 as f32;
                    let dy = ((lparam as i32 >> 16) & 0xFFFF) as i16 as f32;
                    let slot = &*p;
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        handle_rbutton_up(root, hwnd, x, y);
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Hotkey routing. SAFETY: slot pointer null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if p.is_null() {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
                let slot = &*p;
                match app_root() {
                    Some(root) => handle_keydown(hwnd, wparam as u32, msg, root, slot, lparam),
                    None => DefWindowProcW(hwnd, msg, wparam, lparam),
                }
            }
        }
        WM_CHAR => unsafe {
            let p = get_slot_ptr(hwnd);
            if p.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let slot = &*p;
            if slot.kind == WindowKind::ZoneEditor {
                if let Some(root) = app_root() {
                    handle_zone_editor_char(root, wparam as u32);
                    request_redraw(hwnd);
                    return 0;
                }
            }
            if slot.kind == WindowKind::ItemFileRename {
                if let Some(root) = app_root() {
                    handle_item_file_rename_char(root, wparam as u32);
                    request_redraw(hwnd);
                    return 0;
                }
            }
            if slot.kind == WindowKind::RulesWizard {
                if let Some(root) = app_root() {
                    if handle_rules_wizard_char(root, wparam as u32) {
                        request_redraw(hwnd);
                        return 0;
                    }
                }
            }
            if slot.kind == WindowKind::BulkManager {
                if let Some(root) = app_root() {
                    if handle_bulk_manager_char(root, wparam as u32) {
                        request_redraw(hwnd);
                        return 0;
                    }
                }
            }
            if slot.kind == WindowKind::Search {
                if let Some(root) = app_root() {
                    if handle_search_char(root, wparam as u32, hwnd) {
                        request_redraw(hwnd);
                        return 0;
                    }
                }
            }
            if slot.kind == WindowKind::Main {
                if let Some(root) = app_root() {
                    if handle_inline_zone_search_char(root, wparam as u32, hwnd) {
                        request_redraw(hwnd);
                        return 0;
                    }
                    // M7 — desktop_path / watch values live edit (focused-field
                    // model) is tried first, then the passphrase capture path.
                    if handle_settings_text_char(root, wparam as u32) {
                        request_redraw(hwnd);
                        return 0;
                    }
                    if handle_settings_passphrase_char(root, wparam as u32) {
                        request_redraw(hwnd);
                        return 0;
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
        WM_NCHITTEST => {
            // Top toolbar band acts as a drag handle (HTCAPTION).
            unsafe {
                let p = get_slot_ptr(hwnd);
                if p.is_null() {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
                let slot = &*p;
                let root = match app_root() {
                    Some(r) => r,
                    None => return DefWindowProcW(hwnd, msg, wparam, lparam),
                };
                let (sx, sy) = (
                    (lparam as i32 & 0xFFFF) as i16 as i32,
                    ((lparam as i32 >> 16) & 0xFFFF) as i16 as i32,
                );
                let mut pt = POINT { x: sx, y: sy };
                ScreenToClient(hwnd, &mut pt);
                let app = root.app.borrow();
                let dpi = slot.state.dpi.get();
                let lx = bentodesk_style::dpi::device_to_logical_f32(pt.x as f32, dpi);
                let ly = bentodesk_style::dpi::device_to_logical_f32(pt.y as f32, dpi);
                let kind = if slot.kind == WindowKind::Main {
                    ui::main_nchittest_kind(&app, &slot.state, lx, ly)
                } else {
                    ui::nchittest_kind(&app, &slot.state, lx, ly)
                };
                use windows_sys::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};
                match kind {
                    ui::HitKind::Caption => HTCAPTION as LRESULT,
                    ui::HitKind::Client => HTCLIENT as LRESULT,
                    ui::HitKind::Transparent => HTTRANSPARENT as LRESULT,
                }
            }
        }
        m if m == WM_TRAY_ICON => {
            // Ruling B — tray icon callback. lparam carries a Win32 mouse
            // event in its low word, including NOTIFYICON_VERSION_4 callbacks.
            if let Some(command) = tray_command_for_callback(wparam, lparam)
                && let Some(root) = app_root()
            {
                log_static(
                    format!(
                        "tray: callback lparam={} command={}\n",
                        (lparam as u32) & 0xFFFF,
                        command.variant_name()
                    )
                    .as_str(),
                );
                match command {
                    Command::ShowTrayMenu => {
                        // SAFETY: TrackPopupMenu is entered directly from
                        // the tray callback so the menu remains reachable
                        // while the main HWND is hidden and cannot produce a
                        // paint-driven dispatcher drain.
                        unsafe { show_tray_menu(root, hwnd) };
                    }
                    other => {
                        root.dispatcher.push(other);
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_HOTKEY => {
            if let Some(root) = app_root() {
                if let Some(command) = global_hotkey_command(root, wparam as i32) {
                    let quit_requested = command == hotkey::HotkeyCommand::QuitApp;
                    log_static(
                        format!("hotkey: id={} command={command:?}\n", wparam as i32).as_str(),
                    );
                    dispatch_hotkey_command(root, command);
                    consume_dispatcher(root, hwnd);
                    if quit_requested {
                        // `consume_dispatcher` has synchronously performed the
                        // full persistence + DestroyWindow teardown. Two real
                        // cross-process WM_HOTKEY runs still left the idle
                        // message loop resident after WM_QUIT, so make the
                        // explicit QuitApp shortcut terminal rather than
                        // relying on a second pump signal.
                        std::process::exit(0);
                    }
                    request_redraw(hwnd);
                } else {
                    tracing::warn!(
                        target: "bentodesk::hotkey",
                        id = wparam as i32,
                        "WM_HOTKEY received for unknown id"
                    );
                }
            }
            0
        }
        WM_DESTROY => {
            // Clear GWLP_USERDATA BEFORE unregister so any in-flight dispatch
            // on this HWND from the OS message queue sees null and returns
            // early (vs following a freed pointer).
            // SAFETY: state freed via registry; tray removed; PostQuitMessage canonical.
            unsafe {
                KillTimer(hwnd, GHOST_PASSTHROUGH_TIMER_ID);
                KillTimer(hwnd, BACKEND_EVENT_POLL_TIMER_ID);
                KillTimer(hwnd, HOVER_FRAME_TIMER_ID);
                KillTimer(hwnd, STARTUP_MEMORY_TRIM_TIMER_ID);
                KillTimer(hwnd, RESIDENT_MEMORY_TRIM_TIMER_ID);
                KillTimer(hwnd, STACK_TRAY_MEMORY_TRIM_TIMER_ID);
                KillTimer(hwnd, CONTEXT_MENU_INPUT_TIMER_ID);
                if let Err(e) = bentodesk_backend::drag_drop::unregister_drop_target(hwnd as *mut _)
                {
                    tracing::warn!(
                        target: "bentodesk::drag_drop",
                        error = %e,
                        "RevokeDragDrop failed during main-window teardown"
                    );
                }
                if let Some(root) = app_root() {
                    unregister_tray_icon(root, hwnd);
                    set_slot_ptr(hwnd, ptr::null_mut());
                    unregister_global_hotkeys(root, hwnd);
                    // Final save attempt before teardown.
                    let app = root.app.borrow();
                    if !app.zones_path.as_os_str().is_empty() && app.dirty.get() {
                        let _ = storage::write_zones_atomic(&app.zones_path, &app.zones);
                    }
                    drop(app);
                    let _ = root.registry.borrow_mut().unregister(hwnd);
                } else {
                    set_slot_ptr(hwnd, ptr::null_mut());
                }
                PostQuitMessage(0);
            }
            0
        }
        // SAFETY: defaulting unhandled messages.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
