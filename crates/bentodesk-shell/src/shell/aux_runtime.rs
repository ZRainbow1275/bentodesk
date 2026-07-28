//! Native shell owner: `aux_runtime`.

use super::*;

/// Lazily construct an auxiliary HWND of `kind` and pre-build its
/// `WindowSlot` so the first WM_PAINT pumps straight through the existing
/// paint path without the seed branch (which is reserved for the Main
/// window). Returns the new HWND, or `None` if creation failed (e.g. §11
/// R7 cap refused a 9th MiniBar). Idempotent: a second call for the same
/// kind returns the existing HWND without re-creating.
///
/// Per spec §1, aux HWNDs use `WS_EX_LAYERED` or `WS_EX_NOREDIRECTIONBITMAP`
/// alongside `WS_POPUP`, selected by `bentodesk_platform::ex_style_for(kind)`.
/// The bitmask choice is delegated to the platform crate so the §4.1
/// NoRedirectionBitmap-vs-Layered mutex stays in one place.
pub(super) fn ensure_aux_window(root: &AppRoot, kind: WindowKind) -> Option<HWND> {
    if kind == WindowKind::Main {
        return find_main_hwnd(root);
    }
    hide_superseded_workspace_aux_windows(root, kind);
    if let Some(h) = find_aux_window(root, kind) {
        center_aux_window_for_open(h, kind);
        return Some(h);
    }

    let class = aux_class_for(kind);
    let title = aux_title_for(kind);
    let owner = find_main_hwnd(root).unwrap_or(ptr::null_mut());

    let desc = WindowDesc::for_kind(class, title, Some(aux_wnd_proc), kind);
    let hwnd = match create_window(&desc, owner) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::aux_window",
                ?kind,
                error = %e,
                "create_window failed for auxiliary kind"
            );
            return None;
        }
    };

    // Every self-painted focusable auxiliary starts hidden and receives its
    // final monitor-aware rect before Renderer::create. This prevents a native
    // caption/0,0 flash and creates the swapchain at the actual client size.
    center_aux_window_for_open(hwnd, kind);

    // Pre-build the per-window slot now (vs the lazy first-paint path used
    // by Main) so we can stash the slot pointer in GWLP_USERDATA before the
    // OS dispatches the first WM_PAINT. The paint() function's first-window
    // seed branch (`registry.is_empty()`) is already gated on Main being
    // registered first, so aux slots arriving after Main bypass it.
    let mut rect: RECT = unsafe { core::mem::zeroed() };
    // SAFETY: GetClientRect with a freshly-created HWND is canonical.
    unsafe { GetClientRect(hwnd, &mut rect) };
    let w = (rect.right - rect.left).max(1) as u32;
    let h = (rect.bottom - rect.top).max(1) as u32;
    let renderer = match Renderer::create(to_windows_hwnd(hwnd), w, h) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                target: "bentodesk::aux_window",
                ?kind,
                error = %e,
                "Renderer::create failed for auxiliary kind"
            );
            // Best-effort cleanup: destroy the HWND so we don't leak it.
            // SAFETY: hwnd was just created and never published; DestroyWindow
            //         is safe to call on a window owned by the calling thread.
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
                DestroyWindow(hwnd);
            }
            return None;
        }
    };

    let mut win = WindowState::new();
    // Mc-1a — DPI soft-loaded via crate::dpi (GetProcAddress), no static import.
    let dpi = bentodesk_platform::dpi::get_dpi_for_window(hwnd);
    let dpi = if dpi == 0 { 96 } else { dpi };
    win.dpi.set(dpi);
    win.monitors = bentodesk_platform::enumerate_monitors();

    let slot = WindowSlot::new(hwnd, kind, win, renderer);
    let raw = match root.registry.borrow_mut().register(slot) {
        Some(p) => p,
        None => {
            // §11 R7 — registry refused (only the MiniBar cap can refuse
            // today). The HWND was created but is unreachable from the
            // registry, so destroy it to avoid a paint-pump dangling pointer.
            // SAFETY: hwnd owned by this thread, never published into
            //         GWLP_USERDATA on this branch.
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
                DestroyWindow(hwnd);
            }
            return None;
        }
    };
    // SAFETY: stash the registry-stable slot pointer for the wndproc to find.
    unsafe { set_slot_ptr(hwnd, raw) };

    Some(hwnd)
}

/// Window procedure for non-Main HWNDs. Mirrors `wnd_proc` minus tray-icon
/// registration (Main owns the only NIM_ADD) and minus PostQuitMessage on
/// WM_DESTROY (only Main quitting the app is correct — closing a picker
/// must not tear down the process).
pub(super) unsafe extern "system" fn aux_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: WM_NCCREATE ABI guarantees lparam is a CREATESTRUCTW.
            let _cs = unsafe { &*(lparam as *const CREATESTRUCTW) };
            // SAFETY: DefWindowProc must run for WM_NCCREATE per docs.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_PAINT => {
            // SAFETY: paint handles its own slot lookup + null-guard.
            unsafe {
                match paint(hwnd) {
                    Ok(()) => {}
                    // Mc-2b — route an aux window's device loss into the same
                    // recovery driver as Main (recreate the shared chain + this
                    // window's renderer; other windows self-heal on next paint).
                    Err(bentodesk_app::RenderError::DeviceLost) => {
                        if let Some(root) = app_root() {
                            handle_device_lost(root, hwnd);
                        }
                    }
                    Err(e) => log_paint_err(e),
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
            0
        }
        WM_DPICHANGED => {
            // PER_MONITOR_AWARE_V2 — same handling as Main.
            // SAFETY: slot pointer + lparam pointer null-checked below.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let new_dpi = (wparam as u32) & 0xFFFF;
                    let slot = &mut *p;
                    slot.state.dpi.set(new_dpi);
                    slot.state.monitors = bentodesk_platform::enumerate_monitors();
                    if !(lparam as *const RECT).is_null() {
                        // SAFETY: WM_DPICHANGED ABI guarantees lParam is a
                        //         valid pointer to a RECT for the dispatch.
                        let r = &*(lparam as *const RECT);
                        SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        let new_w = (r.right - r.left).max(1) as u32;
                        let new_h = (r.bottom - r.top).max(1) as u32;
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
            // SAFETY: slot pointer fetched from window data — null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let slot = &mut *p;
                    slot.state.monitors = bentodesk_platform::enumerate_monitors();
                    // Mc-3 #13 — rescue a window stranded entirely offscreen by
                    // a monitor unplug. Per-aux code owns normal geometry;
                    // clamp_window_to_monitors no-ops unless the window is off
                    // ALL work areas, so a visible window is never moved.
                    // SWP_NOSIZE keeps the aux's own size.
                    let mut wr: RECT = core::mem::zeroed();
                    if GetWindowRect(hwnd, &mut wr) != 0 {
                        let w = (wr.right - wr.left).max(1);
                        let h = (wr.bottom - wr.top).max(1);
                        let (nx, ny) = bentodesk_platform::clamp_window_to_monitors(
                            wr.left,
                            wr.top,
                            w,
                            h,
                            &slot.state.monitors,
                        );
                        if nx != wr.left || ny != wr.top {
                            SetWindowPos(
                                hwnd,
                                ptr::null_mut(),
                                nx,
                                ny,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
            0
        }
        WM_ACTIVATE => {
            // A context menu owns keyboard focus while open. Losing activation
            // is the native outside-click signal; dismiss only this auxiliary
            // surface and leave every other owned window untouched.
            if (wparam as u32 & 0xFFFF) == WA_INACTIVE {
                unsafe {
                    let p = get_slot_ptr(hwnd);
                    if !p.is_null() && (*p).kind == WindowKind::ContextMenu {
                        if let Some(root) = app_root() {
                            close_context_menu_surface(root);
                        }
                        return 0;
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SHOWWINDOW => {
            // T-099 hibernation entry — eligible for non-Main kinds, so the
            // swap chain releases ~500 ms after the window is hidden.
            // SAFETY: slot pointer fetched from window data — null-checked.
            let hidden_kind = unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let visible = wparam != 0;
                    let slot = &*p;
                    slot.set_visible(visible, GetTickCount());
                    (!visible).then_some(slot.kind)
                } else {
                    None
                }
            };
            if hidden_kind.is_some_and(|kind| kind != WindowKind::Tooltip) {
                if let Some(root) = app_root() {
                    // Tooltips belong to their anchor surface. Hiding any
                    // focusable auxiliary must also retire the tooltip HWND;
                    // otherwise its 200×40 dark host survives as the reported
                    // stray "little black bar" on the desktop.
                    hide_tooltip(root);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_NCHITTEST => unsafe {
            let p = get_slot_ptr(hwnd);
            if p.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let slot = &*p;
            let root = match app_root() {
                Some(root) => root,
                None => return DefWindowProcW(hwnd, msg, wparam, lparam),
            };
            let mut point = POINT {
                x: (lparam as i32 & 0xFFFF) as i16 as i32,
                y: ((lparam as i32 >> 16) & 0xFFFF) as i16 as i32,
            };
            ScreenToClient(hwnd, &mut point);
            let dpi = slot.state.dpi.get();
            let x = bentodesk_style::dpi::device_to_logical_f32(point.x as f32, dpi);
            let y = bentodesk_style::dpi::device_to_logical_f32(point.y as f32, dpi);
            let kind = with_window_slot_viewport(root, slot, || {
                let app = root.app.borrow();
                match slot.kind {
                    WindowKind::ContextMenu => app
                        .active_context_menu
                        .borrow()
                        .as_ref()
                        .map(|session| {
                            if popover::context_menu_contains(session, x, y) {
                                ui::HitKind::Client
                            } else {
                                ui::HitKind::Transparent
                            }
                        })
                        .unwrap_or(ui::HitKind::Transparent),
                    WindowKind::Settings => {
                        ui::settings_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    WindowKind::Search => {
                        ui::search_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    WindowKind::About => {
                        ui::about_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    WindowKind::ZoneEditor => {
                        ui::zone_editor_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    WindowKind::BulkManager => {
                        ui::bulk_manager_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    WindowKind::IconPicker
                    | WindowKind::CapsulePicker
                    | WindowKind::PalettePicker
                    | WindowKind::RulesWizard
                    | WindowKind::ItemFileRename
                    | WindowKind::Suggestor
                    | WindowKind::Timeline
                    | WindowKind::SnapshotPicker => {
                        ui::auxiliary_panel_nchittest_kind(window_slot_logical_viewport(slot), x, y)
                    }
                    _ => ui::nchittest_kind(&app, &slot.state, x, y),
                }
            });
            use windows_sys::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};
            match kind {
                ui::HitKind::Caption => HTCAPTION as LRESULT,
                ui::HitKind::Client => HTCLIENT as LRESULT,
                ui::HitKind::Transparent => HTTRANSPARENT as LRESULT,
            }
        },
        WM_TIMER => {
            if wparam == SETTINGS_OUTSIDE_CLICK_TIMER_ID {
                if let Some(root) = app_root() {
                    poll_settings_outside_click(root, hwnd);
                }
                return 0;
            }
            if wparam == HOVER_FRAME_TIMER_ID {
                handle_hover_frame_timer(hwnd);
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_MOUSEMOVE => {
            // Auxiliary HWNDs own production hover/tooltip paths too. Routing
            // through the shared handler keeps pickers, editors, search, rules,
            // bulk manager, timeline, and MiniBar behavior equivalent to Main.
            // SAFETY: slot pointer fetched from window data — null-checked.
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
                        with_window_slot_viewport(root, slot, || {
                            handle_mouse_move(root, slot, x, y);
                        });
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
                    let handled = with_window_slot_viewport(root, slot, || {
                        if root.app.borrow().active_context_menu.borrow().is_some()
                            && handle_context_menu_mousewheel(root, hwnd, wparam)
                        {
                            return true;
                        }
                        handle_settings_mousewheel(root, slot, hwnd, wparam)
                    });
                    if handled {
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
                    let dpi = slot.state.dpi.get();
                    let x = bentodesk_style::dpi::device_to_logical_f32(dx, dpi);
                    let y = bentodesk_style::dpi::device_to_logical_f32(dy, dpi);
                    if let Some(root) = app_root() {
                        with_window_slot_viewport(root, slot, || {
                            handle_lbutton_down(root, slot, hwnd, x, y);
                        });
                        // Auxiliary pointer surfaces are command producers just
                        // like Main. Reduce the queue before repainting so an
                        // editor Save or Settings click cannot leave one stale
                        // visible frame waiting for a later Main-window input.
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
                        with_window_slot_viewport(root, slot, || {
                            handle_lbutton_up(root, slot, hwnd, x, y);
                        });
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                    }
                }
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Auxiliary HWND keyboard controls are production producers:
            // IconPicker Enter/Esc, BulkManager F3, Search query navigation,
            // Timeline/Snapshot shortcuts, RulesWizard flow, etc.
            // SAFETY: slot pointer null-checked.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if p.is_null() {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
                let slot = &*p;
                match app_root() {
                    Some(root) => {
                        let result = with_window_slot_viewport(root, slot, || {
                            handle_keydown(hwnd, wparam as u32, msg, root, slot, lparam)
                        });
                        // Enter/Escape handlers on editor/search/rules surfaces
                        // may enqueue their business command and return early.
                        consume_dispatcher(root, hwnd);
                        result
                    }
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
                        // QuerySearch must be applied per character; otherwise
                        // Enter can race a batch of stale queued query states.
                        consume_dispatcher(root, hwnd);
                        request_redraw(hwnd);
                        return 0;
                    }
                }
            }
            // W3 (#7 fix wave 2026-06-01) — the Settings section lives on this
            // aux HWND (it holds focus after `SetForegroundWindow`), so its
            // WM_CHAR must route into the §2 text-field + §10 passphrase capture
            // handlers exactly like the Main WM_CHAR block. Without this branch,
            // typing into the desktop-path / watch / passphrase fields fell to
            // DefWindowProc and did NOTHING (the latent bug this fix closes).
            if slot.kind == WindowKind::Settings {
                if let Some(root) = app_root() {
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
        WM_DESTROY => {
            // Drop the per-window slot but DO NOT PostQuitMessage — only
            // the Main window's destruction quits the app. The wndproc
            // GWLP_USERDATA is cleared first so any in-flight dispatch
            // sees null and bails before the slot is freed.
            // SAFETY: registry borrow + set_slot_ptr are canonical.
            unsafe {
                set_slot_ptr(hwnd, ptr::null_mut());
                if let Some(root) = app_root() {
                    let _ = root.registry.borrow_mut().unregister(hwnd);
                }
            }
            0
        }
        // SAFETY: defaulting unhandled messages.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Drain the dispatcher and apply each command's side effects.
///
/// Wave F1.3 — every `Command` variant now has an explicit arm; the match is
/// exhaustive at compile time (no `_ =>` catch-all). Variants that belong to
/// future waves (F2 tooltip / context-menu, F4 picker windows, F5 stack
/// series + items, F1.1 SetSetting, F1.4 ReorderZone / GroupingApply) emit a
/// single `tracing::warn!` per occurrence and delegate via the inline
/// comment — per `feedback_compiles_clean_stub_during_multi_agent_coord.md`,
/// a doc-only no-op stub is preferable to `todo!()` while sibling agents
/// finalise their bridge surfaces.
pub(super) fn reset_settings_transient_state(app: &AppState) {
    app.settings_keybindings_open.set(false);
    // M1h — `settings_plugins_open` removed (Plugins is inline, no modal).
    app.settings_plugin_uninstall_confirm.set(None);
    app.settings_owned_dialog_release_guard.set(false);
    app.settings_keybinding_recording.borrow_mut().take();
    app.set_settings_encryption_mode_hover(None);
    app.set_settings_close_hover(false);
    // W-minor (#7 fix wave) — clear the focused-field caret so a stale focus
    // (and its blinking caret) never persists across Settings opens.
    app.settings_focused_field
        .set(bentodesk_app::SettingsTextField::None);
    // Tauri unmounts Settings on dismiss, so a later open starts at the top.
    // The reusable native HWND must explicitly reset the retained scroll Cell.
    app.scroll_offset_y.set(0.0);
}

pub(super) fn show_settings_surface(root: &AppRoot) -> bool {
    {
        let app = root.app.borrow();
        app.settings_open.set(true);
        // M1a 2026-05-29 — snapshot every persisted General-section toggle
        // BEFORE the user can mutate any of them. Cancel/Escape/Close ×
        // replay this back so cancelled edits never leak into a Save flush.
        // Idempotent: re-opening Settings (e.g., toggling visibility from
        // tray) re-captures fresh values, dropping any stale snapshot from
        // a previously dismissed-but-not-saved session. `settings_dirty`
        // resets here too so the Save button reads as clean on each open.
        *app.settings_snapshot.borrow_mut() = Some(app.snapshot_settings());
        app.settings_dirty.set(false);
        app.settings_save_error.borrow_mut().take();
        app.set_settings_encryption_mode_hover(None);
        app.set_settings_close_hover(false);
    }
    // M1i — populate the §2 Paths desktop-source list on open (mirrors Tauri's
    // `getDesktopSources()` on mount) so the dynamic read-only cards reflect the
    // real resolved sources on first paint. Runs the same path as the Refresh
    // (`↻`) button.
    refresh_desktop_sources(root);
    // M1e — refresh the cached Stealth §7 status snapshot on open so the card
    // (and its conditional retry/error/OneDrive rows) reflect the live probe.
    refresh_stealth_status(root);
    // M1g — populate the Backup §9 list on open (mirrors Tauri `BackupCard`'s
    // `onMount → refresh`) so the backup-list isn't empty on first paint. Reads
    // the real rotated vault files via the existing list command/fn; sets
    // `settings_backup_status` to a success/error line the card renders.
    run_settings_backup_list(root);
    // M1h — populate the Plugins §11 list on open (mirrors Tauri's
    // `loadPlugins()` on mount) so the inline plugin cards reflect the real
    // registry on first paint. Reads installed plugins via the existing
    // `refresh_settings_plugins_for_root` (→ `list_plugins_for_root`). A normal
    // successful refresh stays visually quiet; only real lifecycle feedback or
    // an error occupies the inline status row.
    match refresh_settings_plugins_for_root(root) {
        Ok(_changed) => {
            root.app.borrow().settings_plugin_status.borrow_mut().take();
        }
        Err(error) => {
            set_plugin_setting_error(
                root,
                localized_plugin_message(
                    bentodesk_style::i18n_zh_cn::ids::PLUGIN_STATUS_LIST_FAILED_PREFIX,
                    error,
                ),
            );
        }
    }
    // Round-2 RC-2 — DO NOT mount the K1 `business::settings::panel` widget
    // subtree. The new dark shell is hand-painted by `draw_settings_panel`
    // against pure-function rects from `crate::settings_panel`. Mounting the
    // K1 panel + 5 cards adds them to `app.tree` which the Main HWND walks
    // via `draw_node`, leaking the K1 card chrome (Encryption / Keybindings
    // list / Updater frequency / Backup entries / Stealth status) onto the
    // desktop next to the live aux Settings HWND. Ruling B mandates the K1
    // helper / dispatch / rect functions stay alive as ORPHANS for binary
    // compile, but the widget-tree mount is the one K1 hook that has a
    // visible side-effect — keep it inert until M4 deletes the card files
    // outright.
    if let Some(target) = ensure_aux_window(root, WindowKind::Settings) {
        {
            let app = root.app.borrow();
            // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
            let now_ms = unsafe { GetTickCount() };
            app.start_settings_open_animation(now_ms);
        }
        center_settings_aux_window(target);
        // SAFETY: target is the live, process-owned Settings HWND.
        unsafe {
            ShowWindow(target, SW_SHOW);
        }
        focus_window_for_keyboard(target);
        arm_settings_outside_click_timer(target);
        arm_hover_frame_timer(target);
        request_redraw(target);
        true
    } else {
        false
    }
}

pub(super) fn close_settings_surface(root: &AppRoot) -> bool {
    {
        let app = root.app.borrow();
        app.settings_open.set(false);
        reset_settings_transient_state(&app);
    }
    // Settings hover tooltips are anchored to the aux surface. Clear them
    // before the panel disappears so the main surface cannot inherit a stale
    // "Save settings" tip after Save/Cancel/Close.
    hide_tooltip(root);
    if let Some(target) = find_aux_window(root, WindowKind::Settings) {
        // SAFETY: target is a process-owned Settings HWND.
        unsafe {
            KillTimer(target, SETTINGS_OUTSIDE_CLICK_TIMER_ID);
            ShowWindow(target, SW_HIDE);
        }
        request_redraw(target);
        true
    } else {
        false
    }
}
