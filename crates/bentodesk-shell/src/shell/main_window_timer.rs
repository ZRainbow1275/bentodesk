//! Main-window timer routing.

use super::*;

pub(super) fn handle_main_window_timer(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if wparam == CONTEXT_MENU_INPUT_TIMER_ID {
        if let Some(root) = app_root() {
            poll_context_menu_input(root, hwnd);
        }
        return 0;
    }
    if wparam == GHOST_PASSTHROUGH_TIMER_ID {
        unsafe {
            let p = get_slot_ptr(hwnd);
            if !p.is_null() {
                let slot = &*p;
                if let Some(root) = app_root()
                    && let Some((x, y, passthrough)) =
                        refresh_ghost_cursor_passthrough(root, slot, hwnd)
                {
                    if !passthrough {
                        handle_mouse_move(root, slot, x, y);
                        request_redraw(hwnd);
                    } else {
                        // V-8.5 (2026-05-21) — passthrough flipped
                        // ON means the cursor is now over blank
                        // pixels (or off the overlay entirely).
                        // `handle_mouse_move` won't fire from here
                        // and the OS does NOT route a fresh
                        // `WM_MOUSEMOVE` once `WS_EX_TRANSPARENT`
                        // is hot. Without this branch the pill
                        // hover state stays pinned at scale 1.04
                        // and never plays the 220 ms ease-out
                        // recover. Calling `clear_hover` here
                        // fires `update_pill_hover_animator(None)`
                        // which starts the recover tween for the
                        // previously-hovered pill, and the
                        // `tick_pill_animator` per-frame pump
                        // keeps redrawing until it settles.
                        let app = root.app.borrow();
                        let had_hover = app.hovered_zone.get().is_some();
                        drop(app);
                        if had_hover {
                            clear_hover(root);
                            arm_hover_frame_timer(hwnd);
                            request_redraw(hwnd);
                        }
                    }
                }
            }
        }
        return 0;
    }
    if wparam == TRAY_ICON_RETRY_TIMER_ID {
        unsafe {
            KillTimer(hwnd, TRAY_ICON_RETRY_TIMER_ID);
            if let Some(root) = app_root() {
                register_tray_icon(root, hwnd);
            }
        }
        return 0;
    }
    if wparam == BACKEND_EVENT_POLL_TIMER_ID {
        if let Some(root) = app_root() {
            let hover_changed = unsafe {
                let p = get_slot_ptr(hwnd);
                !p.is_null() && reconcile_main_hover_from_cursor(root, &*p, hwnd)
            };
            let backend_changed = drain_backend_events(root);
            // The backend bridge already wakes every 250 ms. Reuse it
            // for the empty inline-search idle dismissal instead of
            // adding a permanent search-only timer.
            let search_changed =
                close_idle_inline_zone_search(root, hwnd, unsafe { GetTickCount() });
            if hover_changed || backend_changed || search_changed {
                flush_dirty_zones(root);
                request_redraw(hwnd);
            }
        }
        return 0;
    }
    if wparam == HOVER_FRAME_TIMER_ID {
        handle_hover_frame_timer(hwnd);
        return 0;
    }
    if wparam == STARTUP_MEMORY_TRIM_TIMER_ID {
        unsafe {
            KillTimer(hwnd, STARTUP_MEMORY_TRIM_TIMER_ID);
        }
        trim_runtime_memory("startup-delayed");
        return 0;
    }
    if wparam == RESIDENT_MEMORY_TRIM_TIMER_ID {
        // A hidden auxiliary surface may receive no later WM_PAINT,
        // so its deferred backbuffer release cannot rely on the paint
        // pump alone. Reuse this existing idle-memory checkpoint.
        if let Some(root) = app_root() {
            flush_hibernation(root, unsafe { GetTickCount() });
        }
        trim_runtime_memory("resident-idle");
        return 0;
    }
    if wparam == STACK_TRAY_MEMORY_TRIM_TIMER_ID {
        unsafe {
            KillTimer(hwnd, STACK_TRAY_MEMORY_TRIM_TIMER_ID);
        }
        trim_runtime_memory("stack-tray-open");
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
