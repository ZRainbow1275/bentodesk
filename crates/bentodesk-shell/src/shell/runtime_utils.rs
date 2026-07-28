//! Native shell owner: `runtime_utils`.

use super::*;

/// Q4 ruling — fixed-size, stack-only UTF-16 widener.
pub(super) fn widen_static<const N: usize>(s: &str) -> [u16; N] {
    let mut out = [0u16; N];
    if N == 0 {
        return out;
    }
    let cap = N - 1;
    for (i, u) in s.encode_utf16().enumerate() {
        if i >= cap {
            break;
        }
        out[i] = u;
    }
    out
}

pub(super) fn widen_dynamic(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) fn log_static(msg: &str) {
    let mut stderr = std::io::stderr();
    let _ = std::io::Write::write_all(&mut stderr, msg.as_bytes());
    let _ = std::io::Write::flush(&mut stderr);
}

pub(super) fn compact_process_heaps() {
    let mut heaps: [HANDLE; 32] = [ptr::null_mut(); 32];
    // SAFETY: `heaps` is a valid writable stack buffer. `GetProcessHeaps`
    // writes at most `heaps.len()` handles and returns the required count.
    let count = unsafe { GetProcessHeaps(heaps.len() as u32, heaps.as_mut_ptr()) };
    let count = (count as usize).min(heaps.len());
    for heap in heaps
        .iter()
        .copied()
        .take(count)
        .filter(|heap| !heap.is_null())
    {
        // SAFETY: handles come directly from GetProcessHeaps for this process.
        // Compact failure/unsupported LFH heaps are non-fatal.
        unsafe {
            let _ = HeapCompact(heap, 0);
        }
    }
}

pub(super) fn trim_runtime_memory(reason: &str) {
    // 1) Drop D2D effect / glyph cache that can be rebuilt on demand.
    if let Ok(f) = bentodesk_platform::d2d::factory() {
        // SAFETY: factory() returns a process-static reference; ClearResources
        // is documented as re-entrant-safe and failure is not observable here.
        unsafe { f.device.ClearResources(0) };
    }
    // 2) Let the DXGI device shed idle graphics allocations if the interface is available.
    let _ = bentodesk_platform::d3d::trim();
    // 3) Release retained mimalloc segments from startup-only work.
    bentodesk_platform::allocator::collect_retained_segments();
    // 4) Decommit compactable process-heap slack left by startup-only Win32/COM.
    compact_process_heaps();
    // 5) Push cold pages to standby.
    // SAFETY: GetCurrentProcess returns a kernel pseudo-handle;
    // EmptyWorkingSet failure (FALSE) is non-fatal.
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
    log_static(format!("memory: trim reason={reason}\n").as_str());
}

pub(super) fn arm_resident_memory_trim(hwnd: HWND) {
    // SAFETY: `hwnd` is owned by the UI thread. The timer posts WM_TIMER back
    // to the same window and is intentionally periodic until killed.
    unsafe {
        let timer = SetTimer(
            hwnd,
            RESIDENT_MEMORY_TRIM_TIMER_ID,
            RESIDENT_MEMORY_TRIM_MS,
            None,
        );
        if timer == 0 {
            log_static(
                format!(
                    "memory: SetTimer(RESIDENT_MEMORY_TRIM) failed (GetLastError={})\n",
                    GetLastError()
                )
                .as_str(),
            );
        }
    }
}

pub(super) fn arm_stack_tray_memory_trim(hwnd: HWND) {
    // SAFETY: `hwnd` is owned by the UI thread. Resetting the same id before
    // arming keeps repeated StackTray opens to a single pending trim.
    unsafe {
        KillTimer(hwnd, STACK_TRAY_MEMORY_TRIM_TIMER_ID);
        let timer = SetTimer(
            hwnd,
            STACK_TRAY_MEMORY_TRIM_TIMER_ID,
            STACK_TRAY_MEMORY_TRIM_MS,
            None,
        );
        if timer == 0 {
            log_static(
                format!(
                    "memory: SetTimer(STACK_TRAY_MEMORY_TRIM) failed (GetLastError={})\n",
                    GetLastError()
                )
                .as_str(),
            );
        }
    }
}

/// Mc-1b — show a modal, user-visible error box. Under
/// `windows_subsystem="windows"` stderr is NULL, so this is the only channel
/// a normal (non-debugger) user can see. §11-clean: builds NUL-terminated
/// UTF-16 buffers without any unwrap/expect/panic, and `MessageBoxW` with a
/// null owner cannot itself panic. Safe to call from the panic hook.
pub(super) fn show_fatal_box(title: &str, body: &str) {
    let mut title_w: Vec<u16> = title.encode_utf16().collect();
    title_w.push(0);
    let mut body_w: Vec<u16> = body.encode_utf16().collect();
    body_w.push(0);
    // SAFETY: both buffers are NUL-terminated; a null owner HWND is valid and
    // documented for an ownerless message box.
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// V-10 (2026-05-21) — Snapshot the Main HWND's GWL_EXSTYLE bitmask and log
/// whether `WS_EX_TRANSPARENT` / `WS_EX_TOPMOST` are present. Used by the
/// startup audit thread to correlate user-reported "can't click desktop" to
/// the actual Win32 state at well-known moments (t+0/100/500/2000ms after
/// `ghost_layer::attach`).
///
/// SAFETY: `hwnd` MUST be the live Main HWND. The OS guarantees the handle
/// stays valid for the process lifetime; `GetWindowLongPtrW` accepts a stale
/// handle by returning 0 (no UB).
pub(super) unsafe fn log_main_ex_style_audit(hwnd: HWND, label: &str) {
    // WS_EX_TRANSPARENT = 0x20, WS_EX_TOPMOST = 0x08 — see windows-sys
    // `WindowsAndMessaging` constants. Hard-coded here to avoid adding
    // imports just for the audit logger.
    const WS_EX_TRANSPARENT_BIT: isize = 0x0000_0020;
    const WS_EX_TOPMOST_BIT: isize = 0x0000_0008;
    // SAFETY: documented above on the function signature.
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let has_trans = (ex & WS_EX_TRANSPARENT_BIT) != 0;
    let has_top = (ex & WS_EX_TOPMOST_BIT) != 0;
    log_static(
        format!(
            "v10_audit: {label} hwnd=0x{:X} ex_style=0x{:08X} WS_EX_TRANSPARENT={} WS_EX_TOPMOST={}\n",
            hwnd as usize, ex as u32, has_trans, has_top,
        )
        .as_str(),
    );
}

pub(super) fn request_redraw(hwnd: HWND) {
    // SAFETY: InvalidateRect with null rect = whole client area.
    unsafe {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

pub(super) fn request_theme_surface_redraw(root: &AppRoot, animate_transition: bool) {
    let registry = root.registry.borrow();
    for slot in registry.iter() {
        if !slot.is_visible.get() {
            continue;
        }
        if animate_transition && slot.kind == WindowKind::Settings {
            arm_hover_frame_timer(slot.hwnd);
        }
        request_redraw(slot.hwnd);
    }
}

pub(super) fn hover_frame_pump_needed(app: &AppState) -> bool {
    // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
    let now_ms = unsafe { GetTickCount() };
    app.hover_scheduler.get().is_pending()
        || stack_bloom_animation_active(app)
        || app.pill_animator.borrow().is_active(now_ms)
        || app.settings_open_animation_pending_at(now_ms)
        || app.theme_transition_pending_at(now_ms)
}

pub(super) fn stack_bloom_animation_active(app: &AppState) -> bool {
    app.stack_bloom_anchor.get().is_some()
        && (app.stack_bloom_leaving.get() || app.stack_bloom_progress.get() < 1.0)
}

pub(super) fn stack_bloom_cursor_watch_active(app: &AppState) -> bool {
    app.stack_bloom_anchor.get().is_some() && !app.stack_bloom_leaving.get()
}

pub(super) fn main_hover_cursor_watch_active(app: &AppState) -> bool {
    app.hovered_zone.get().is_some() || stack_bloom_cursor_watch_active(app)
}

pub(super) fn hover_frame_timer_needed(app: &AppState) -> bool {
    hover_frame_pump_needed(app) || main_hover_cursor_watch_active(app)
}

pub(super) fn arm_hover_frame_timer(hwnd: HWND) {
    // SAFETY: `hwnd` is an HWND owned by this UI thread. The timer has no
    // callback; WM_TIMER is routed through `wnd_proc`.
    unsafe {
        let timer = SetTimer(hwnd, HOVER_FRAME_TIMER_ID, HOVER_FRAME_POLL_MS, None);
        if timer == 0 {
            log_static(
                format!(
                    "hover_frame: SetTimer failed (GetLastError={})\n",
                    GetLastError()
                )
                .as_str(),
            );
        }
    }
}

pub(super) fn handle_hover_frame_timer(hwnd: HWND) {
    if let Some(root) = app_root() {
        let mut interaction_changed = false;
        // SAFETY: `hwnd` owns a registry-stable WindowSlot for its lifetime;
        // GetTickCount is total and thread-safe.
        let (renderer_animating, renderer_settled) = unsafe {
            let slot = get_slot_ptr(hwnd);
            if slot.is_null() {
                (false, false)
            } else {
                let now_ms = GetTickCount();
                let animating = (*slot).renderer.auxiliary_open_animation_pending(now_ms);
                let settled =
                    !animating && (*slot).renderer.settle_auxiliary_open_animation(now_ms);
                (animating, settled)
            }
        };
        let watch_main_cursor = {
            let app = root.app.borrow();
            main_hover_cursor_watch_active(&app)
        };
        if watch_main_cursor {
            // Once blank pixels switch the Main HWND to WS_EX_TRANSPARENT,
            // Windows no longer routes the follow-up WM_MOUSEMOVE that would
            // close a stable Zone surface. Reuse this already-armed timer as a
            // cursor sentinel; no idle redraw loop is introduced.
            unsafe {
                let p = get_slot_ptr(hwnd);
                if !p.is_null() {
                    let slot = &*p;
                    if let Some((x, y, passthrough)) =
                        refresh_ghost_cursor_passthrough(root, slot, hwnd)
                    {
                        if passthrough {
                            clear_hover(root);
                        } else {
                            // The window may have been transparent while the
                            // cursor crossed a family gap, so no WM_MOUSEMOVE is
                            // guaranteed on petal re-entry. Sample the same
                            // paint-priority hover path directly from this
                            // already-running cursor sentinel.
                            let app = root.app.borrow();
                            // SAFETY: GetTickCount has no failure mode.
                            let now_ms = GetTickCount();
                            interaction_changed |=
                                update_main_zone_hover_for_point(&app, x, y, now_ms);
                        }
                    }
                }
            }
        }
        {
            let app = root.app.borrow();
            // SAFETY: GetTickCount has no failure mode.
            let now_ms = unsafe { GetTickCount() };
            interaction_changed |= poll_stack_bloom_interaction(&app, now_ms);
        }
        if interaction_changed || renderer_settled {
            request_redraw(hwnd);
        }
        let app = root.app.borrow();
        if hover_frame_pump_needed(&app) || renderer_animating {
            drop(app);
            request_redraw(hwnd);
        } else if main_hover_cursor_watch_active(&app) {
            // Stable Zone hover: keep only the cursor sentinel alive. Blank
            // transparent pixels otherwise suppress the leave WM_MOUSEMOVE.
            drop(app);
        } else {
            drop(app);
            // SAFETY: Killing an absent timer is a harmless no-op for this HWND/id.
            unsafe {
                KillTimer(hwnd, HOVER_FRAME_TIMER_ID);
            }
        }
    } else {
        // SAFETY: Killing an absent timer is a harmless no-op for this HWND/id.
        unsafe {
            KillTimer(hwnd, HOVER_FRAME_TIMER_ID);
        }
    }
}

pub(super) fn sync_hover_frame_timer(hwnd: HWND, app: &AppState, renderer_animating: bool) {
    if hover_frame_timer_needed(app) || renderer_animating {
        arm_hover_frame_timer(hwnd);
    } else {
        // SAFETY: Killing an absent timer is a harmless no-op for this HWND/id.
        unsafe {
            KillTimer(hwnd, HOVER_FRAME_TIMER_ID);
        }
    }
}

pub(super) fn log_paint_err(e: bentodesk_app::RenderError) {
    use core::fmt::Write as _;
    let mut buf: smallvec::SmallVec<[u8; 256]> = smallvec::SmallVec::new();
    struct SvWriter<'a>(&'a mut smallvec::SmallVec<[u8; 256]>);
    impl core::fmt::Write for SvWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            self.0.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }
    let _ = writeln!(SvWriter(&mut buf), "paint err: {e}");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), &buf);
}

pub(super) unsafe fn get_slot_ptr(hwnd: HWND) -> *mut WindowSlot {
    // SAFETY: GWLP_USERDATA is process-private; cast back to original type.
    //         T-010: per-HWND storage is now `*mut WindowSlot` (was
    //         `*mut WinState` pre-multi-window-pivot).
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowSlot }
}

pub(super) unsafe fn set_slot_ptr(hwnd: HWND, p: *mut WindowSlot) {
    // SAFETY: GWLP_USERDATA write is process-private.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, p as isize);
    }
}

// -----------------------------------------------------------------------------
// Mc-2b — device-recovery driver (shell side).
// -----------------------------------------------------------------------------

/// Maximum device-chain recreate attempts allowed inside one [`RECOVERY_WINDOW`]
/// before the shell gives up and shows the fatal box. Tuned high enough to ride
/// out a transient TDR/driver-reset burst but low enough to stop a recreate
/// storm against a permanently-dead adapter.
pub(super) const MAX_RECOVERY_ATTEMPTS: u32 = 3;
/// Rolling window over which [`MAX_RECOVERY_ATTEMPTS`] are counted. A loss after
/// a clean stretch longer than this restarts the streak rather than escalating.
pub(super) const RECOVERY_WINDOW: Duration = Duration::from_secs(60);

/// Mc-2b — react to a `RenderError::DeviceLost` raised by `paint()` or a
/// `resize()`. The pure decision (`decide_recovery`) lives in the platform
/// crate; this routine owns the [`RecoveryState`](bentodesk_platform::RecoveryState)
/// stored on `AppRoot` plus the `Instant`-based retry window, recreates the
/// process-wide device chain, and rebuilds THIS window's renderer.
///
/// §11: never panics — a failed recreate is logged and counted, and the retry
/// cap (or the next device-lost frame) decides whether to escalate. §10: cold —/// only reached on the rare device-lost event, never on a healthy frame.
///
/// SAFETY: `hwnd` must be the live HWND whose paint/resize raised the loss; the
/// slot pointer is fetched the same way `paint()` does (`get_slot_ptr`).
pub(super) unsafe fn handle_device_lost(root: &AppRoot, hwnd: HWND) {
    let now = Instant::now();
    let within_window = root
        .last_recovery_at
        .get()
        .is_some_and(|t| now.duration_since(t) < RECOVERY_WINDOW);
    let state = root.recovery_state.get();

    match bentodesk_platform::decide_recovery(state, within_window, MAX_RECOVERY_ATTEMPTS) {
        bentodesk_platform::RecoveryAction::BeginRecreate => {
            // Transition the attempt counter: a fresh streak (Healthy, or the
            // window elapsed) starts at 1; an in-window retry increments.
            let prev_attempts = match state {
                bentodesk_platform::RecoveryState::Recovering { attempts } if within_window => {
                    attempts
                }
                _ => 0,
            };
            let attempts = prev_attempts + 1;
            root.recovery_state
                .set(bentodesk_platform::RecoveryState::Recovering { attempts });
            root.last_recovery_at.set(Some(now));

            // The old swap chain's frame-latency waitable handle dies with the
            // recreate. Deregister it (null) so the message loop falls back to
            // its timed wait for the single synchronous beat until
            // `WindowComp::create` (inside `rebuild_after_device_loss`)
            // re-registers the fresh handle.
            bentodesk_platform::message_loop::register_frame_handle(ptr::null_mut());

            // Recreate the process-wide device chain ONCE, then rebuild this
            // window's renderer on it. Other windows self-heal via the
            // generation check at the top of their next paint (Impl B).
            // SAFETY: slot pointer fetched + null-checked like `paint()`.
            let recovered = bentodesk_platform::recover_device_chain().and_then(|()| {
                let p = unsafe { get_slot_ptr(hwnd) };
                if p.is_null() {
                    // No slot yet (loss before first paint built one): the
                    // chain is fresh, so the lazy paint path will create the
                    // renderer on it. Nothing per-window to rebuild.
                    return Ok(());
                }
                // SAFETY: non-null per the guard above; single-threaded pump.
                let slot = unsafe { &mut *p };
                slot.renderer.rebuild_after_device_loss().map_err(|e| {
                    bentodesk_platform::PlatformError::Init(match e {
                        bentodesk_app::RenderError::DeviceLost => {
                            "renderer rebuild_after_device_loss: device still lost"
                        }
                        _ => "renderer rebuild_after_device_loss failed",
                    })
                })
            });

            match recovered {
                Ok(()) => {
                    // The chain + this renderer are healthy again. Clear the
                    // paint failure streak so the recoverable loss never trips
                    // the fatal threshold, and repaint this window. (The state
                    // stays `Recovering` until a clean frame in the chokepoint
                    // resets it to `Healthy`.)
                    PAINT_FAIL_STREAK.store(0, Ordering::Relaxed);
                    // SAFETY: InvalidateRect with null rect = whole client area.
                    unsafe {
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                    log_static("device-recovery: chain rebuilt + renderer recreated\n");
                }
                Err(e) => {
                    // The attempt is counted; the next DeviceLost frame either
                    // retries (still inside the window, under the cap) or gives
                    // up. DO NOT crash here — log only.
                    tracing::warn!(
                        target: "bentodesk::recovery",
                        error = %e,
                        "device-recovery attempt failed"
                    );
                }
            }
        }
        bentodesk_platform::RecoveryAction::GiveUp => {
            root.recovery_state
                .set(bentodesk_platform::RecoveryState::GaveUp);
            show_fatal_box(
                "Display device lost",
                "BentoDesk could not recover the graphics device after repeated attempts. The application will now close.",
            );
            // SAFETY: PostQuitMessage just enqueues WM_QUIT. Match the nonzero
            // exit code style the PAINT fatal path uses (`PostQuitMessage(2)`).
            unsafe {
                PostQuitMessage(2);
            }
        }
        bentodesk_platform::RecoveryAction::Ignore => {
            // Already gave up — avoid recreate storms. Nothing to do.
        }
    }
}
