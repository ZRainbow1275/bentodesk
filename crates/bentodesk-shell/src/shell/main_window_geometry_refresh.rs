use super::*;

pub(super) fn cancel_main_client_gestures(root: &AppRoot) -> bool {
    let pending_drag_out = root.pending_item_drag_out.borrow_mut().take().is_some();
    let pending_stack_bloom = root.pending_stack_drop_bloom.replace(None).is_some();
    let app = root.app.borrow();
    let dragged_zone = app.zone_drag.get().map(|(id, _, _)| id);
    let cancelled = app.cancel_viewport_gestures();
    if cancelled || pending_drag_out || pending_stack_bloom {
        // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
        reset_pointer_drag_hover_channels(&app, dragged_zone, unsafe { GetTickCount() });
    }
    cancelled || pending_drag_out || pending_stack_bloom
}

/// Cancel only when capture was lost with a custom gesture still in flight.
pub(super) fn cancel_main_client_gestures_after_capture_loss(root: &AppRoot) -> bool {
    let pending_drag_out = root.pending_item_drag_out.borrow().is_some();
    let active = {
        let app = root.app.borrow();
        app.zone_drag.get().is_some()
            || app.zone_resize.get().is_some()
            || app.item_drag.borrow().is_some()
            || app.stack_tray_drag.get().is_some()
    };
    if !pending_drag_out && !active {
        return false;
    }
    cancel_main_client_gestures(root)
}

pub(super) fn prepare_main_zone_geometry_refresh(root: &AppRoot, slot: &WindowSlot, hwnd: HWND) {
    slot.state.schedule_zone_geometry_normalize();
    let was_resize = root.app.borrow().zone_resize.get().is_some();
    let cancelled = cancel_main_client_gestures(root);
    if cancelled {
        // SAFETY: a work-area transition invalidates the Main-client gesture
        // coordinates, so releasing this thread's current capture is required.
        unsafe { ReleaseCapture() };
    }
    if was_resize {
        restore_default_main_cursor(root, slot, hwnd);
    }
    request_redraw(hwnd);
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod workarea_message_tests {
    use super::*;

    #[test]
    fn only_main_display_dpi_and_setworkarea_messages_schedule_geometry_refresh() {
        for msg in [WM_DISPLAYCHANGE, WM_DPICHANGED] {
            assert!(message_requires_main_zone_geometry_refresh(
                WindowKind::Main,
                msg,
                0,
            ));
            assert!(!message_requires_main_zone_geometry_refresh(
                WindowKind::Settings,
                msg,
                0,
            ));
        }
        assert!(message_requires_main_zone_geometry_refresh(
            WindowKind::Main,
            WM_SETTINGCHANGE,
            SPI_SETWORKAREA as WPARAM,
        ));
        assert!(!message_requires_main_zone_geometry_refresh(
            WindowKind::Main,
            WM_SETTINGCHANGE,
            0,
        ));
    }
}

#[inline]
pub(super) fn message_requires_main_zone_geometry_refresh(
    kind: WindowKind,
    msg: u32,
    wparam: WPARAM,
) -> bool {
    kind == WindowKind::Main
        && (msg == WM_DISPLAYCHANGE
            || msg == WM_DPICHANGED
            || (msg == WM_SETTINGCHANGE && wparam as u32 == SPI_SETWORKAREA))
}

pub(super) fn handle_window_dpi_changed(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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

            let target_rect = if slot.kind == WindowKind::Main {
                Some(bentodesk_platform::main_window_rect())
            } else if !(lparam as *const RECT).is_null() {
                // SAFETY: WM_DPICHANGED ABI guarantees lParam is a
                // valid pointer for the duration of message dispatch.
                let r = &*(lparam as *const RECT);
                Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
            } else {
                None
            };
            if let Some((target_x, target_y, target_w, target_h)) = target_rect {
                let new_w = target_w.max(1) as u32;
                let new_h = target_h.max(1) as u32;
                if message_requires_main_zone_geometry_refresh(slot.kind, WM_DPICHANGED, wparam)
                    && let Some(root) = app_root()
                {
                    prepare_main_zone_geometry_refresh(root, slot, hwnd);
                }
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    target_x,
                    target_y,
                    target_w.max(1),
                    target_h.max(1),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                if slot.kind == WindowKind::Main {
                    slot.renderer.mark_backdrop_dirty();
                }
                // T-012 — rebuild swap chain at new monitor's pixel
                // density. `Renderer::resize` re-passes the swap
                // chain flags so the FRAME_LATENCY_WAITABLE_OBJECT
                // doesn't get demoted (Wave 12 contract).
                // Mc-2b / #10 — route a device loss on this resize into
                // recovery instead of discarding it.
                if let Err(bentodesk_app::RenderError::DeviceLost) =
                    slot.renderer.resize(new_w, new_h)
                    && let Some(root) = app_root()
                {
                    handle_device_lost(root, hwnd);
                }
            }
        }
    }
    0
}
