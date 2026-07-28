//! Native shell owner: `aux_positioning`.

use super::*;

/// Find the first registered HWND of `kind`. Returns `None` if none exist
/// (caller chains into `create_aux_window` to lazy-spawn). Aux windows are
/// kept in the same `WindowRegistry` as Main, so the §11 R7 cap on MiniBar
/// keeps applying.
pub(super) fn find_aux_window(root: &AppRoot, kind: WindowKind) -> Option<HWND> {
    root.registry
        .borrow()
        .iter()
        .find(|s| s.kind == kind)
        .map(|s| s.hwnd)
}

/// Find the first Main HWND in the registry — the natural owner for every
/// auxiliary popup / picker / tooltip. Returns `None` only during early
/// startup (between `create_window` and the first `paint` call seeding the
/// Main slot), in which case the caller falls back to a top-level window
/// (`null_mut()` parent).
pub(super) fn find_main_hwnd(root: &AppRoot) -> Option<HWND> {
    root.registry
        .borrow()
        .iter()
        .find(|s| s.kind == WindowKind::Main)
        .map(|s| s.hwnd)
}

/// Focusable workspace tools are mutually exclusive.  They are separate
/// native HWNDs (rather than pages inside one web overlay), so leaving an old
/// tool visible while centring the next one makes the two rounded dialogs sit
/// exactly on top of each other.  The result looks like a nested browser modal
/// with duplicate titles and close buttons.  Pickers are intentionally not
/// triggers: they may sit above their ZoneEditor/BulkManager parent while the
/// user chooses a value.  A subsequent workspace-tool open does dismiss any
/// stale picker along with the superseded tool.
#[inline]
pub(super) fn is_workspace_aux_surface(kind: WindowKind) -> bool {
    matches!(
        kind,
        WindowKind::RulesWizard
            | WindowKind::BulkManager
            | WindowKind::ZoneEditor
            | WindowKind::ItemFileRename
            | WindowKind::Suggestor
            | WindowKind::Timeline
            | WindowKind::SnapshotPicker
            | WindowKind::Search
    )
}

#[inline]
pub(super) fn hides_when_workspace_aux_opens(kind: WindowKind) -> bool {
    is_workspace_aux_surface(kind)
        || matches!(
            kind,
            WindowKind::IconPicker | WindowKind::CapsulePicker | WindowKind::PalettePicker
        )
}

pub(super) fn hide_superseded_workspace_aux_windows(root: &AppRoot, next_kind: WindowKind) {
    if !is_workspace_aux_surface(next_kind) {
        return;
    }

    // Copy handles before calling ShowWindow: WM_SHOWWINDOW may re-enter the
    // UI pump, so never hold the registry RefCell borrow across the Win32 call.
    let handles = root
        .registry
        .borrow()
        .iter()
        .filter(|slot| slot.kind != next_kind && hides_when_workspace_aux_opens(slot.kind))
        .map(|slot| slot.hwnd)
        .collect::<smallvec::SmallVec<[HWND; 12]>>();
    for hwnd in handles {
        // SAFETY: every handle came from this process-owned WindowRegistry.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

#[inline]
pub(super) fn tooltip_uses_aux_surface(
    _anchor: bentodesk_app::WindowHandle,
    _main_anchor: Option<bentodesk_app::WindowHandle>,
    _context_menu_open: bool,
) -> bool {
    // Tooltip text must never allocate a second native HWND beside an already
    // self-contained panel. The old auxiliary surface outlived hover changes
    // and appeared as the detached black strip reported below Bulk Manager,
    // Editor, Settings and Suggestor. Inline labels/status own that feedback.
    false
}

pub(super) fn settings_aux_host_rect(
    work: bentodesk_platform::RectI32,
    dpi: u32,
) -> (i32, i32, i32, i32) {
    // Settings is an ordinary panel-sized native popup, not a draggable
    // work-area overlay.  Returning the complete work area here made
    // HTCAPTION move a screen-sized HWND beyond the monitor while the painted
    // card appeared to detach from its backdrop.  Keep the Tauri 480-DIP card
    // and 80-vh height contract, but make those dimensions the HWND itself.
    let scale = dpi.max(96) as f32 / 96.0;
    let work_logical_height = work.height().max(1) as f32 / scale;
    let logical_height = bentodesk_app::settings_panel::SETTINGS_PANEL_HEIGHT_MAX
        .min(work_logical_height * bentodesk_app::settings_panel::SETTINGS_PANEL_MAX_WORKAREA_FRAC);
    centered_fixed_aux_host_rect(
        work,
        dpi,
        bentodesk_app::settings_panel::SETTINGS_PANEL_WIDTH_M1,
        logical_height,
    )
}

pub(super) fn center_settings_aux_window(hwnd: HWND) {
    let mut cursor = POINT { x: 0, y: 0 };
    let monitor = if unsafe { GetCursorPos(&mut cursor) } != 0 {
        bentodesk_platform::monitor_from_point(cursor.x, cursor.y)
    } else {
        bentodesk_platform::primary_monitor()
    };
    let work = monitor.rect_work;
    // Move first so GetDpiForWindow resolves the target monitor on mixed-DPI
    // desktops, matching the About-window path below.
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            work.left,
            work.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd).max(96);
    let (x, y, width, height) = settings_aux_host_rect(work, dpi);
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn center_about_aux_window(hwnd: HWND) {
    let mut cursor = POINT { x: 0, y: 0 };
    let monitor = if unsafe { GetCursorPos(&mut cursor) } != 0 {
        bentodesk_platform::monitor_from_point(cursor.x, cursor.y)
    } else {
        bentodesk_platform::primary_monitor()
    };
    let work = monitor.rect_work;

    // Move onto the target monitor before asking Windows for the effective
    // per-monitor DPI. This keeps 640×520 DIPs stable on mixed-DPI desktops.
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            work.left,
            work.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd).max(96);
    let scale = dpi as f32 / 96.0;
    let width = ((bentodesk_app::business::about::WINDOW_WIDTH * scale).round() as i32)
        .clamp(1, work.width().max(1));
    let height = ((bentodesk_app::business::about::WINDOW_HEIGHT * scale).round() as i32)
        .clamp(1, work.height().max(1));
    let x = work.left + (work.width() - width) / 2;
    let y = work.top + (work.height() - height) / 2;
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn centered_fixed_aux_host_rect(
    work: bentodesk_platform::RectI32,
    dpi: u32,
    logical_width: f32,
    logical_height: f32,
) -> (i32, i32, i32, i32) {
    let scale = dpi.max(96) as f32 / 96.0;
    let width = ((logical_width * scale).round() as i32).clamp(1, work.width().max(1));
    let height = ((logical_height * scale).round() as i32).clamp(1, work.height().max(1));
    (
        work.left + (work.width() - width) / 2,
        work.top + (work.height() - height) / 2,
        width,
        height,
    )
}

pub(super) fn center_fixed_aux_window(hwnd: HWND, kind: WindowKind) {
    let mut cursor = POINT { x: 0, y: 0 };
    let monitor = if unsafe { GetCursorPos(&mut cursor) } != 0 {
        bentodesk_platform::monitor_from_point(cursor.x, cursor.y)
    } else {
        bentodesk_platform::primary_monitor()
    };
    let work = monitor.rect_work;

    // Resolve per-monitor DPI after moving the hidden borderless host onto the
    // monitor where the user invoked the auxiliary surface.
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            work.left,
            work.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd).max(96);
    let (logical_width, logical_height) = default_size(kind);
    let (x, y, width, height) =
        centered_fixed_aux_host_rect(work, dpi, logical_width as f32, logical_height as f32);
    unsafe {
        SetWindowPos(
            hwnd,
            ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub(super) fn center_aux_window_for_open(hwnd: HWND, kind: WindowKind) {
    match kind {
        WindowKind::Settings => center_settings_aux_window(hwnd),
        WindowKind::About => center_about_aux_window(hwnd),
        WindowKind::IconPicker
        | WindowKind::CapsulePicker
        | WindowKind::PalettePicker
        | WindowKind::RulesWizard
        | WindowKind::BulkManager
        | WindowKind::ZoneEditor
        | WindowKind::ItemFileRename
        | WindowKind::Suggestor
        | WindowKind::Timeline
        | WindowKind::SnapshotPicker
        | WindowKind::Search => center_fixed_aux_window(hwnd, kind),
        WindowKind::Main
        | WindowKind::ContextMenu
        | WindowKind::Tooltip
        | WindowKind::DragPreview
        | WindowKind::MiniBar => {}
    }
}

pub(super) fn arm_settings_outside_click_timer(hwnd: HWND) {
    // Consume the click that opened Settings so the first poll cannot dismiss
    // the freshly shown panel. Subsequent low-bit transitions represent new
    // clicks even when Explorer keeps the desktop rather than a normal app as
    // the foreground window.
    unsafe {
        let _ = GetAsyncKeyState(0x01);
        SetTimer(
            hwnd,
            SETTINGS_OUTSIDE_CLICK_TIMER_ID,
            SETTINGS_OUTSIDE_CLICK_POLL_MS,
            None,
        );
    }
}

pub(super) fn arm_settings_owned_dialog_release_guard(root: &AppRoot) {
    root.app
        .borrow()
        .settings_owned_dialog_release_guard
        .set(true);
}

pub(super) fn settings_panel_client_device_rect(client: RECT, dpi: u32) -> RECT {
    let scale = bentodesk_style::dpi::scale_factor(dpi);
    let viewport = bentodesk_style::Size {
        width: bentodesk_style::dpi::device_to_logical_f32(
            (client.right - client.left).max(0) as f32,
            dpi,
        ),
        height: bentodesk_style::dpi::device_to_logical_f32(
            (client.bottom - client.top).max(0) as f32,
            dpi,
        ),
    };
    let panel = bentodesk_app::settings_panel::settings_panel_rect_m1(viewport);
    RECT {
        left: client.left + (panel.x * scale).round() as i32,
        top: client.top + (panel.y * scale).round() as i32,
        right: client.left + (panel.right() * scale).round() as i32,
        bottom: client.top + (panel.bottom() * scale).round() as i32,
    }
}

#[inline]
pub(super) fn settings_outside_click_should_close(
    target_is_settings: bool,
    target_is_main: bool,
    target_is_same_process: bool,
    point_inside_panel: bool,
) -> bool {
    !target_is_same_process || target_is_main || (target_is_settings && !point_inside_panel)
}

#[inline]
pub(super) fn settings_owned_dialog_guard_transition(
    armed: bool,
    currently_pressed: bool,
) -> (bool, bool) {
    if armed {
        // Suppress this poll; remain armed only while the accepting/cancelling
        // mouse press is still physically held.
        (true, currently_pressed)
    } else {
        (false, false)
    }
}

pub(super) fn poll_settings_outside_click(root: &AppRoot, hwnd: HWND) {
    let left_button_state = unsafe { GetAsyncKeyState(0x01) as u16 };
    let pressed_since_last_poll = left_button_state & 0x0001 != 0;
    let currently_pressed = left_button_state & 0x8000 != 0;
    {
        let app = root.app.borrow();
        let (suppress, keep_armed) = settings_owned_dialog_guard_transition(
            app.settings_owned_dialog_release_guard.get(),
            currently_pressed,
        );
        if suppress {
            app.settings_owned_dialog_release_guard.set(keep_armed);
            // The GetAsyncKeyState read above also consumes the low-bit click
            // transition that accepted/cancelled the owned picker. Never treat
            // that dialog-space click as a Settings outside click.
            return;
        }
    }
    if !pressed_since_last_poll && !currently_pressed {
        return;
    }

    let mut point = POINT { x: 0, y: 0 };
    let mut client: RECT = unsafe { core::mem::zeroed() };
    if unsafe { GetCursorPos(&mut point) } == 0 || unsafe { GetClientRect(hwnd, &mut client) } == 0
    {
        return;
    }
    let dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd).max(96);
    let panel = settings_panel_client_device_rect(client, dpi);
    let mut panel_top_left = POINT {
        x: panel.left,
        y: panel.top,
    };
    let mut panel_bottom_right = POINT {
        x: panel.right,
        y: panel.bottom,
    };
    if unsafe { ClientToScreen(hwnd, &mut panel_top_left) } == 0
        || unsafe { ClientToScreen(hwnd, &mut panel_bottom_right) } == 0
    {
        return;
    }
    let point_inside_panel = point.x >= panel_top_left.x
        && point.x < panel_bottom_right.x
        && point.y >= panel_top_left.y
        && point.y < panel_bottom_right.y;

    // Keep same-process native dialogs/pickers usable. A desktop or unrelated
    // application click is observed without intercepting it, then dismisses
    // Settings on this timer tick.
    let target = unsafe { WindowFromPoint(point) };
    let mut target_process_id = 0_u32;
    if !target.is_null() {
        unsafe { GetWindowThreadProcessId(target, &mut target_process_id) };
    }
    if !settings_outside_click_should_close(
        target == hwnd,
        find_main_hwnd(root) == Some(target),
        target_process_id == std::process::id(),
        point_inside_panel,
    ) {
        return;
    }

    cancel_settings_general(root);
    close_settings_surface(root);
    log_static("settings: close reason=outside_click\n");
}
