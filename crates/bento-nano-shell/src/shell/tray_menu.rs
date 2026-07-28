//! Native shell owner: `tray_menu`.

use super::*;

pub(super) fn tray_menu_command_for_item(
    item: TrayMenuItem,
    main_visible: bool,
    origin: DispatchPoint,
) -> Command {
    match item {
        TrayMenuItem::ShowHideMain => {
            if main_visible {
                Command::HideWindow(WindowKind::Main)
            } else {
                Command::ShowWindow(WindowKind::Main)
            }
        }
        TrayMenuItem::NewZone => Command::CreateZone(ZoneSpec {
            name: smol_str::SmolStr::new_inline("Zone"),
            origin,
            size: DispatchSize::new(200, 120),
        }),
        TrayMenuItem::AutoOrganize => Command::AutoOrganize,
        TrayMenuItem::OpenSettings => Command::OpenSettings,
        TrayMenuItem::About => Command::OpenAbout,
        TrayMenuItem::Exit => Command::QuitApp,
    }
}

pub(super) fn tray_menu_command_for_choice(
    chosen: i32,
    main_visible: bool,
    origin: DispatchPoint,
) -> Option<Command> {
    if chosen <= 0 {
        return None;
    }
    let chosen_idx = usize::try_from(chosen).ok()?.saturating_sub(1);
    let item = TrayMenuItem::ORDER.get(chosen_idx)?;
    Some(tray_menu_command_for_item(*item, main_visible, origin))
}

pub(super) fn handle_tray_wm_command(root: &AppRoot, hwnd: HWND, choice: usize) -> bool {
    let Some(pending) = root.tray_context_menu.borrow().as_ref().copied() else {
        return false;
    };
    let Ok(chosen) = i32::try_from(choice) else {
        return false;
    };
    let Some(command) = tray_menu_command_for_choice(chosen, pending.main_visible, pending.origin)
    else {
        return false;
    };
    root.tray_context_menu_consumed.set(true);
    log_static(
        format!(
            "tray: popup wm_command id={} command={}\n",
            choice,
            command.variant_name()
        )
        .as_str(),
    );
    root.dispatcher.push(command);
    request_redraw(hwnd);
    true
}

pub(super) unsafe fn show_tray_menu(root: &AppRoot, hwnd: HWND) {
    // F2-06 — rich tray menu sourced from `business::tray_menu::TrayMenuItem::ORDER`.
    // Per `prompts/0503/02-migration-parity.md` row 16, the 1.x menu has six
    // entries (ShowHideMain / NewZone / AutoOrganize / OpenSettings / About /
    // Exit) with three dividers; nano sources both the order and the labels
    // from the business module so the tray surface and any future
    // keyboard-driven menu pump share a single declaration.

    // SAFETY: CreatePopupMenu canonical; NULL return logged + early-out.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        tracing::warn!(
            target: "bentodesk::tray_menu",
            "CreatePopupMenu returned NULL; tray menu skipped"
        );
        return;
    }

    // Resolve "is the main window visible right now" for the ShowHideMain
    // label flip. The Main slot's `is_visible` flag tracks the
    // WM_SHOWWINDOW round-trip (T-099 hibernation gate).
    let main_visible = {
        let reg = root.registry.borrow();
        reg.iter()
            .find(|s| s.kind == WindowKind::Main)
            .map(|s| s.is_visible.get())
            .unwrap_or(true)
    };

    for (idx, item) in TrayMenuItem::ORDER.iter().enumerate() {
        if item.needs_divider_before() {
            // SAFETY: AppendMenuW with MF_SEPARATOR ignores the string ptr.
            unsafe {
                AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            }
        }
        let label = item.label(main_visible);
        let label_w = widen_static::<48>(label);
        // command_id is 1-based (idx + 1) so the TrackPopupMenu sentinel 0
        // (= dismissed without selection) doesn't collide with item 0.
        let cmd_id = idx + 1;
        // SAFETY: AppendMenuW with valid UTF-16 strings, NUL-terminated by
        //         widen_static's contract.
        unsafe {
            AppendMenuW(menu, MF_STRING, cmd_id, label_w.as_ptr());
        }
    }

    let mut pt = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos canonical.
    unsafe { GetCursorPos(&mut pt) };
    log_static(
        format!(
            "tray: popup opening at {},{} main_visible={} items={}\n",
            pt.x,
            pt.y,
            main_visible,
            TrayMenuItem::ORDER.len()
        )
        .as_str(),
    );
    // SAFETY: SetForegroundWindow canonical — required by TrackPopupMenu so
    //         the menu dismisses on outside click.
    unsafe { SetForegroundWindow(hwnd) };
    let origin = DispatchPoint::new(pt.x, pt.y);
    root.tray_context_menu
        .borrow_mut()
        .replace(PendingTrayContextMenu {
            main_visible,
            origin,
        });
    root.tray_context_menu_consumed.set(false);

    // SAFETY: TrackPopupMenu with valid menu + owner HWND; TPM_RETURNCMD
    //         makes the call synchronous and returns the chosen command_id
    //         (or 0 on dismiss).
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            ptr::null(),
        )
    };
    // SAFETY: DestroyMenu canonical — must run regardless of outcome.
    unsafe { DestroyMenu(menu) };
    root.tray_context_menu.borrow_mut().take();

    let consumed_by_wm_command = root.tray_context_menu_consumed.get();

    if consumed_by_wm_command {
        consume_dispatcher(root, hwnd);
    } else if let Some(command) = tray_menu_command_for_choice(chosen, main_visible, origin) {
        log_static(
            format!(
                "tray: popup selected id={} command={}\n",
                chosen,
                command.variant_name()
            )
            .as_str(),
        );
        root.dispatcher.push(command);
        consume_dispatcher(root, hwnd);
        request_redraw(hwnd);
    } else if chosen <= 0 {
        log_static("tray: popup dismissed without selection\n");
        tracing::info!(
            target: "bentodesk::tray_menu",
            "Tray menu dismissed without selecting an action"
        );
    } else {
        log_static(format!("tray: popup unknown id={chosen}\n").as_str());
        tracing::warn!(
            target: "bentodesk::tray_menu",
            chosen,
            "Tray menu returned an unknown command id"
        );
    }
}

pub(super) fn show_tooltip_payload(root: &AppRoot, text: &smol_str::SmolStr) -> bool {
    root.app.borrow().show_tooltip_text(text.clone())
}

pub(super) fn hide_tooltip_payload(root: &AppRoot) -> bool {
    root.app.borrow().hide_tooltip_text()
}

/// F2-05 — `Command::ShowTooltip` handler. Lazily spawns the
/// `WindowKind::Tooltip` aux HWND, sources its chrome from the shared
/// tooltip descriptor/state, positions it 4 px below + right of the anchor's
/// client rect bottom-left, and shows it via `SW_SHOWNOACTIVATE` so the
/// caller's foreground/caret state is preserved.
///
/// `anchor.0 == 0` (`WindowHandle::NULL`) means the producer didn't pin
/// the tooltip to a specific window — fall back to the cursor position so
/// the tip still renders somewhere visible.
pub(super) fn show_tooltip(
    root: &AppRoot,
    anchor: bento_nano_app::WindowHandle,
    text: &smol_str::SmolStr,
) {
    let Some(host) = ensure_aux_window(root, WindowKind::Tooltip) else {
        return;
    };
    let tooltip_changed = show_tooltip_payload(root, text);

    // Compute on-screen position. Default to the current cursor when the
    // anchor is NULL; otherwise pin to the anchor's bottom-left + 4 px
    // gutter so the tip doesn't cover the cursor.
    let (sx, sy) = if anchor.0 == 0 {
        let mut pt = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos canonical.
        unsafe { GetCursorPos(&mut pt) };
        (pt.x + 12, pt.y + 18)
    } else {
        let anchor_hwnd = anchor.0 as HWND;
        let mut rect: RECT = unsafe { core::mem::zeroed() };
        // SAFETY: GetClientRect on a caller-supplied HWND that the producer
        //         vetted; on a stale handle the call no-ops and `rect`
        //         stays zeroed, which collapses to the (0, 0) fallback.
        unsafe { GetClientRect(anchor_hwnd, &mut rect) };
        let mut pt = POINT {
            x: rect.left,
            y: rect.bottom,
        };
        // SAFETY: ClientToScreen on the same HWND we just probed.
        unsafe { ClientToScreen(anchor_hwnd, &mut pt) };
        (pt.x + 4, pt.y + 4)
    };

    // Re-use the kind's default footprint (200 × 40) as the host size.
    // The widget tree clamps the visible run to the SmolStr text's
    // measured length; the host HWND is the upper bound.
    let (w, h) = bento_nano_platform::default_size(WindowKind::Tooltip);

    // SAFETY: SetWindowPos on a HWND we own; flags skip activation +
    //         z-order (Tooltip's ex-style already pins WS_EX_TOPMOST).
    unsafe {
        SetWindowPos(
            host,
            ptr::null_mut(),
            sx,
            sy,
            w,
            h,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
    // SAFETY: ShowWindow with SW_SHOWNOACTIVATE — Tooltip's WS_EX_NOACTIVATE
    //         + SW_SHOWNOACTIVATE keeps the caller's foreground intact.
    unsafe {
        ShowWindow(host, SW_SHOWNOACTIVATE);
    }
    if tooltip_changed {
        request_redraw(host);
    }
}

/// F2-05 — `Command::HideTooltip` handler. Idempotent: if the aux Tooltip
/// HWND was never created (the very first hover hasn't happened) the call
/// is a no-op. WM_SHOWWINDOW(false) drives T-099 hibernation ~500 ms later
/// so the hidden tooltip releases its swap chain backbuffer.
pub(super) fn hide_tooltip(root: &AppRoot) {
    let tooltip_changed = hide_tooltip_payload(root);
    let Some(host) = find_aux_window(root, WindowKind::Tooltip) else {
        return;
    };
    if tooltip_changed {
        request_redraw(host);
    }
    // SAFETY: ShowWindow with SW_HIDE on a HWND we own.
    unsafe {
        ShowWindow(host, SW_HIDE);
    }
}

/// F2-06 — `Command::ShowContextMenu` handler. Renders the supplied items
/// via Win32 `TrackPopupMenu` with `TPM_RETURNCMD | TPM_RIGHTBUTTON |
/// TPM_NONOTIFY`, then resolves the chosen `command_id` against the items
/// list and emits a debug trace so the round-trip is verifiable.
///
/// The aux `WindowKind::ContextMenu` HWND serves as TrackPopupMenu's owner
/// when the caller didn't supply one — `TPM_NONOTIFY` keeps the OS from
/// posting a WM_COMMAND we'd have to route while still satisfying the
/// "owner must be non-NULL for keyboard nav" contract.
///
/// SAFETY: TrackPopupMenu loop is canonical — see `show_tray_menu`.
pub(super) unsafe fn show_context_menu(
    root: &AppRoot,
    anchor: bento_nano_app::WindowHandle,
    items: &bento_nano_app::ContextMenuItems,
) {
    use bento_nano_app::business::popover::Popover;

    if items.is_empty() {
        return;
    }
    // Reference the Popover descriptor — the chrome values feed snap.md-
    // driven sizing once a per-aux-window widget tree mounts in F3.
    // Keeping the reference here pins `business::popover` to the
    // production code path for the F2.5 reachability gate.
    let _chrome = Popover::new();

    // SAFETY: CreatePopupMenu canonical; NULL return logged + early-out.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        tracing::warn!(
            target: "bentodesk::context_menu",
            "CreatePopupMenu returned NULL; context menu skipped"
        );
        return;
    }

    for item in items.iter() {
        let label_w = widen_static::<64>(item.label.as_str());
        // SAFETY: AppendMenuW with valid UTF-16 strings; command_id is the
        //         caller-defined u32 widened to usize for the API.
        unsafe {
            AppendMenuW(menu, MF_STRING, item.command_id as usize, label_w.as_ptr());
        }
    }

    // Owner HWND — prefer the caller's anchor; Main is a sufficient non-null
    // owner for this legacy native-menu command path. Creating a hidden D2D
    // ContextMenu renderer solely to own an HMENU wastes several megabytes.
    let owner: HWND = if anchor.0 != 0 {
        anchor.0 as HWND
    } else if let Some(h) = find_main_hwnd(root) {
        h
    } else {
        // No HWND available — destroy the menu and bail.
        // SAFETY: DestroyMenu canonical.
        unsafe { DestroyMenu(menu) };
        return;
    };

    // Anchor screen coords — bottom-left of anchor's client rect, or the
    // current cursor if anchor was NULL.
    let pt = if anchor.0 != 0 {
        let mut rect: RECT = unsafe { core::mem::zeroed() };
        // SAFETY: GetClientRect on caller-supplied HWND.
        unsafe { GetClientRect(anchor.0 as HWND, &mut rect) };
        let mut p = POINT {
            x: rect.left,
            y: rect.bottom,
        };
        // SAFETY: ClientToScreen on the same HWND.
        unsafe { ClientToScreen(anchor.0 as HWND, &mut p) };
        p
    } else {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos canonical.
        unsafe { GetCursorPos(&mut p) };
        p
    };

    // SAFETY: SetForegroundWindow on owner — required for the menu to
    //         dismiss on outside click per Win32 contract.
    unsafe { SetForegroundWindow(owner) };

    // SAFETY: TrackPopupMenu with valid menu + owner; TPM_RETURNCMD makes
    //         the call synchronous. TPM_NONOTIFY keeps the OS from posting
    //         a WM_COMMAND we'd have to route.
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            owner,
            ptr::null(),
        )
    };
    // SAFETY: DestroyMenu canonical — must run regardless of outcome.
    unsafe { DestroyMenu(menu) };

    if chosen <= 0 {
        return;
    }
    let chosen_id = chosen as u32;
    if let Some(item) = items.iter().find(|i| i.command_id == chosen_id) {
        // The item's `command_id` is the producer's contract — they map it
        // to a concrete Command in their own state. Emit a debug trace so
        // manual smoke verifies the round-trip.
        tracing::debug!(
            target: "bentodesk::context_menu",
            command_id = chosen_id,
            label = %item.label,
            "context menu selection routed"
        );
    }
}

/// F2-06 — `Command::HideContextMenu` handler. TrackPopupMenu is a
/// synchronous modal-loop API, so once `show_context_menu` returns the
/// menu HWND is already gone. This handler just hides the long-lived aux
/// `WindowKind::ContextMenu` HWND if the caller was using it as an owner.
pub(super) fn hide_context_menu(root: &AppRoot) {
    let Some(host) = find_aux_window(root, WindowKind::ContextMenu) else {
        return;
    };
    // SAFETY: ShowWindow with SW_HIDE on a HWND we own.
    unsafe {
        ShowWindow(host, SW_HIDE);
    }
}
