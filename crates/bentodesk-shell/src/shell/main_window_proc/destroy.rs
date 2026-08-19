use super::*;

pub(super) fn handle_main_window_destroy(hwnd: HWND) -> LRESULT {
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
        if let Err(e) = bentodesk_backend::drag_drop::unregister_drop_target(hwnd as *mut _) {
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
