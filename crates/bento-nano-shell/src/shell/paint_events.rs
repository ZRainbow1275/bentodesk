//! Native shell owner: `paint_events`.

use super::*;

// -----------------------------------------------------------------------------
// Paint — lazy-init slot on first frame.
// -----------------------------------------------------------------------------

pub(super) unsafe fn paint(hwnd: HWND) -> Result<(), bento_nano_app::RenderError> {
    let mut slot_ptr = unsafe { get_slot_ptr(hwnd) };
    if slot_ptr.is_null() {
        // First paint for this HWND — build a slot, register it, stash
        // the heap pointer in GWLP_USERDATA.
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: GetClientRect with valid hwnd + out param.
        unsafe { GetClientRect(hwnd, &mut rect) };
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;

        let renderer = Renderer::create(to_windows_hwnd(hwnd), w, h)?;
        let mut win = WindowState::new();
        // Phase 2.3.1a / Ruling 4 — seed the per-window DPI + monitor cache
        // immediately so zone routing / DPI scaling are deterministic from
        // the first paint. Mc-1a — DPI soft-loaded via crate::dpi.
        let dpi = bento_nano_platform::dpi::get_dpi_for_window(hwnd);
        let dpi = if dpi == 0 { 96 } else { dpi };
        win.dpi.set(dpi);
        win.monitors = bento_nano_platform::enumerate_monitors();

        let root = match app_root() {
            Some(r) => r,
            None => return Ok(()),
        };
        // First Main window — also seed AppState viewport + persistence
        // path + initial widget tree. Subsequent windows skip this.
        if root.registry.borrow().is_empty() {
            let mut app = root.app.borrow_mut();
            app.viewport = bento_nano_style::Size {
                width: w as f32,
                height: h as f32,
            };
            if let Ok(p) = storage::appdata_path() {
                app.zones_path = p;
            }
            let _ = ui::mount_main_tree(&mut app);
            let _ = win.run_layout(&app);
        }
        // Register the slot — `register` returns the heap-stable
        // `*mut WindowSlot` we stash in GWLP_USERDATA.
        let slot = WindowSlot::new(hwnd, WindowKind::Main, win, renderer);
        let raw = match root.registry.borrow_mut().register(slot) {
            Some(p) => p,
            // §11 R7 cap refused — Main can't be refused, so this path is
            // dead code today; kept defensively for future re-spawns.
            None => return Ok(()),
        };
        // SAFETY: stash raw pointer in window data.
        unsafe { set_slot_ptr(hwnd, raw) };
        slot_ptr = raw;
    }

    // SAFETY: slot_ptr non-null at this point.
    let slot = unsafe { &mut *slot_ptr };
    let root = match app_root() {
        Some(r) => r,
        None => return Ok(()),
    };

    // 1) Tick animations using elapsed wall-clock since the previous frame.
    // SAFETY: GetTickCount is total + thread-safe.
    let now = unsafe { GetTickCount() };
    let last = root.last_tick_ms.replace(now);
    let dt_ms = now.wrapping_sub(last);
    let dt = (dt_ms as f32) / 1000.0;

    let mut app = root.app.borrow_mut();
    let mut any_active = false;
    // P1 (#7 fix wave 2026-06-01) — keep the frame-pump alive (so this HWND keeps
    // repainting) while ANY settings text field is focused, so the §2/§10 caret
    // can blink at the Windows ~530ms cadence. Without this the pump idles after
    // the keystroke redraw and the caret would freeze ON/OFF.
    if app.settings_focused_field.get() != bento_nano_app::SettingsTextField::None {
        any_active = true;
    }
    let pointer_drag_active = normal_pointer_drag_active(&app);
    if !pointer_drag_active {
        if tick_stack_bloom_animation(&app, now) {
            any_active = true;
        }
        // A3 (2026-05-29) — poll the hover-intent / grace-collapse scheduler on
        // the frame-tick timestamp (no WM_TIMER, fits the immediate-mode pump).
        // When an expand/collapse deadline elapses this flips the Wave G2 morph
        // target, which `tick_zone_pill_animation` below then advances.
        if poll_hover_scheduler(&app, now) {
            any_active = true;
        }
        // A3 — keep the pump alive while a hover/collapse timer is still armed so
        // the deadline is actually reached even if the cursor has gone idle.
        if app.hover_scheduler.get().is_pending() {
            any_active = true;
        }
        // Wave G2 — capsule pill expand/shrink morph.
        if tick_zone_pill_animation(&app, now) {
            any_active = true;
        }
        // V-8 hover / press animator. Keeps the frame-pump alive only while
        // sampled pill visual entries are in flight.
        if tick_pill_animator(&app, now) {
            any_active = true;
        }
        if settle_inline_zone_search_animation(&app, now) {
            any_active = true;
        }
        // M3-A2 — per-item hover/press ramp. Same frame cadence as the pill
        // animator; retires leaving/released cards and keeps the pump alive while
        // a 150ms hover / 80ms press transition is still in flight.
        if tick_item_hover_animator(&app, now) {
            any_active = true;
        }
    } else {
        log_animation_proof_state(&app, "tick_pointer_drag_skip", now, None, None);
    }
    if app.highlight_overlay.borrow_mut().tick(dt_ms) {
        any_active = true;
    }
    if let Some(result) = slot.state.layout.last_result() {
        let ids: smallvec::SmallVec<[NodeId; 16]> = result.iter().map(|(id, _)| *id).collect();
        for id in ids {
            if let Ok(WidgetNode::IconButton(btn)) = app.tree.get_mut(id) {
                if btn.tick(dt) {
                    any_active = true;
                }
            }
        }
    }

    // 2) Spec §C2 — exactly one `flush_dirty` per frame.
    sync_hover_frame_timer(
        hwnd,
        &app,
        slot.renderer.auxiliary_open_animation_pending(now),
    );

    let frame = root.frame_id.get().wrapping_add(1);
    root.frame_id.set(frame);
    let _digest = app.tree.flush_dirty(frame);

    // 3) Render via the per-window slot. Per-window paint err counter is
    //    incremented on failure inside `WindowSlot::paint`.
    //
    // T-099 lift — if hibernation has released the swap chain (e.g. the
    // window was just re-shown), recreate at the cached width/height before
    // we ask the renderer to paint. `ensure_swap_chain` is idempotent
    // (no-op when already resident) so the steady-state cost is one
    // branch + a `Cell::get`.
    if !slot.renderer.is_resident() {
        let w = slot.renderer.width;
        let h = slot.renderer.height;
        if let Err(e) = slot.renderer.ensure_swap_chain(w, h) {
            drop(app);
            slot.note_paint_err();
            return Err(e);
        }
    }
    // Every auxiliary HWND owns an independent swap-chain size. Rendering it
    // with the process-global Main viewport recentres/clips modal geometry
    // inside the wrong coordinate space (the old About window showed only a
    // cropped right-hand slice). Temporarily project the slot's device size to
    // logical DIPs for this frame, then restore Main's viewport so an aux paint
    // cannot poison desktop hit-testing. Main itself keeps its live viewport.
    let previous_viewport = app.viewport;
    app.viewport = window_slot_logical_viewport(slot);
    let r = slot.paint(&mut app);
    if slot.kind != WindowKind::Main {
        app.viewport = previous_viewport;
    }
    drop(app);

    if rehydrate_live_folder_bindings(root) {
        request_redraw(hwnd);
    }

    // 4) Drain dispatcher and apply HWND-bound side effects + change-trigger save.
    consume_dispatcher(root, hwnd);

    // 4b) Drain backend native events into the UI pump. Watcher diagnostics
    // and ghost-layer power-resume repair both run in the selected-stack exe.
    let backend_changed = drain_backend_events(root);
    if backend_changed {
        request_redraw(hwnd);
    }

    // 5) T-099 hibernation pass — release backbuffers for non-Main windows
    //    whose `pending_hibernate` flag has aged past `HIBERNATE_GATE_MS`.
    flush_hibernation(root, now);

    if any_active {
        request_redraw(hwnd);
    }

    // Wave 15 — Tier 0 #29/#31 + #28 one-shot post-first-paint trim.
    if r.is_ok() {
        let win_ref = &slot.state;
        if !win_ref.first_paint_done.get() {
            trim_runtime_memory("first-paint");
            win_ref.first_paint_done.set(true);
        }
    }

    r
}

pub(super) fn drain_desktop_events(root: &AppRoot) -> bool {
    let _watcher_alive = root.desktop_watcher.borrow().is_some();
    let mut drained = 0u32;
    let mut changed = false;
    let mut smart_group_requested = false;
    while let Ok(event) = root.desktop_events.try_recv() {
        drained = drained.saturating_add(1);
        tracing::debug!(
            target: "bentodesk::watcher",
            event_type = %event.event_type,
            path = %event.path,
            old_path = ?event.old_path,
            "desktop watcher event routed to UI pump"
        );
        let mut event_changed = false;
        {
            let mut app = root.app.borrow_mut();
            match event.event_type.as_str() {
                "delete" => {
                    if app.zones.mark_item_missing(&event.path, true) {
                        event_changed = true;
                    }
                }
                "modify" => {
                    let item_path = bento_nano_app::ItemPath::new(event.path.as_str());
                    if let Some(hash) = load_icon_hash_for_path(&item_path) {
                        let zone_ids: Vec<_> = app.zones.iter().map(|zone| zone.id).collect();
                        for zone_id in zone_ids {
                            if app.zones.set_item_icon_hash(
                                zone_id,
                                &event.path,
                                std::borrow::Cow::Owned(hash.clone()),
                            ) {
                                event_changed = true;
                            }
                        }
                    }
                }
                "create" => {
                    smart_group_requested |= app.setting_smart_layout.get();
                    if let Some(old_path) = event.old_path.as_deref() {
                        if app.zones.replace_item_path(
                            old_path,
                            std::borrow::Cow::Owned(event.path.clone()),
                        ) {
                            event_changed = true;
                        }
                    } else if app.zones.mark_item_missing(&event.path, false) {
                        event_changed = true;
                    }
                }
                _ => {}
            }
            if event_changed {
                app.mark_dirty();
            }
        }
        changed |= event_changed;
        match run_on_file_change_rules(root, &event) {
            Ok(outcome) => {
                if outcome.has_visible_status() || outcome.applied_actions() {
                    changed = true;
                }
            }
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::rules",
                    event_type = %event.event_type,
                    path = %event.path,
                    error = %error,
                    "OnFileChange rule execution failed from desktop watcher event"
                );
                set_rules_wizard_error(
                    root,
                    localized_current(
                        format!("文件变更规则执行失败：{error}"),
                        format!("File-change rule execution failed: {error}"),
                    ),
                );
                changed = true;
            }
        }
        if drained >= 32 {
            break;
        }
    }
    if smart_group_requested && auto_organize_desktop(root) {
        changed = true;
    }
    changed || drained > 0
}

pub(super) fn drain_live_folder_events(root: &AppRoot) -> bool {
    let mut drained = 0u32;
    let mut changed = false;
    while let Ok(event) = root.live_folder_events.try_recv() {
        drained = drained.saturating_add(1);
        let folder = live_folder_path_for_zone(root, event.zone_id);
        match refresh_live_folder_zone(root, event.zone_id) {
            Ok(zone_changed) => {
                changed |= zone_changed;
                tracing::debug!(
                    target: "bentodesk::live_folder",
                    zone_id = event.zone_id.0,
                    folder = ?folder,
                    changed = zone_changed,
                    "live folder refresh event routed to UI pump"
                );
            }
            Err(error) => {
                tracing::warn!(
                    target: "bentodesk::live_folder",
                    zone_id = event.zone_id.0,
                    folder = ?folder,
                    error = %error,
                    "live folder refresh failed from watcher event"
                );
                set_live_folder_error(
                    root,
                    localized_current(
                        format!("区域 {} 的绑定文件夹刷新失败：{error}", event.zone_id.0),
                        format!(
                            "Live folder refresh failed for zone {}: {error}",
                            event.zone_id.0
                        ),
                    ),
                );
                changed = true;
            }
        }
        if drained >= 16 {
            break;
        }
    }
    changed || drained > 0
}

pub(super) fn drain_updater_events(root: &AppRoot) -> bool {
    let mut drained = 0u32;
    let mut changed = false;
    while let Ok(event) = root.updater_events.try_recv() {
        let should_auto_download = {
            let app = root.app.borrow();
            let should = updater_event_should_auto_download(&app, &event);
            apply_update_event_to_app(&app, event);
            should
        };
        drained = drained.saturating_add(1);
        changed = true;
        if should_auto_download {
            match root.updater.download() {
                Ok(()) => {}
                Err(error) => {
                    let app = root.app.borrow();
                    set_update_error(
                        &app,
                        "自动下载更新失败",
                        "Automatic update download failed",
                        &error,
                    );
                }
            }
        }
        if drained >= 16 {
            break;
        }
    }
    changed
}

pub(super) fn drain_rules_scheduler_events(root: &AppRoot) -> bool {
    let mut drained = 0u32;
    let mut changed = false;
    while let Ok(event) = root.rules_scheduler_events.try_recv() {
        drained = drained.saturating_add(1);
        match run_interval_rule_for_scheduler_event(root, &event) {
            Ok(outcome) => {
                if outcome.has_visible_status() || outcome.applied_actions() {
                    changed = true;
                }
            }
            Err(error) => {
                let SchedulerEvent::RuleDue { rule_id } = &event;
                tracing::warn!(
                    target: "bentodesk::rules",
                    rule_id = %rule_id,
                    error = %error,
                    "Interval rule execution failed from selected-stack scheduler event"
                );
                set_rules_wizard_error(
                    root,
                    localized_current(
                        format!("定时规则执行失败：{error}"),
                        format!("Interval rule execution failed: {error}"),
                    ),
                );
                changed = true;
            }
        }
        if drained >= 16 {
            break;
        }
    }
    changed || drained > 0
}

pub(super) fn drain_ghost_events(root: &AppRoot) -> bool {
    let mut drained = 0u32;
    while let Ok(event) = root.ghost_events.try_recv() {
        drained = drained.saturating_add(1);
        match event {
            bento_nano_backend::ghost_layer::GhostLayerEvent::PowerResume => {
                schedule_power_resume(root);
            }
        }
        if drained >= 8 {
            break;
        }
    }
    drained > 0
}

pub(super) fn schedule_power_resume(root: &AppRoot) {
    let config = {
        let app = root.app.borrow();
        bento_nano_backend::power::ResumeConfig {
            delay_ms: app.hibernate_resume_delay_ms.get().max(0) as u32,
            safe_start_enabled: app.safe_start_after_hibernation.get(),
        }
    };
    bento_nano_backend::power::handle_resume(config, root.power_event_tx.clone());
}

pub(super) fn drain_power_events(root: &AppRoot) -> bool {
    let mut drained = 0u32;
    while let Ok(event) = root.power_events.try_recv() {
        drained = drained.saturating_add(1);
        match event {
            bento_nano_backend::power::PowerEvent::Resumed => {
                let snapshot = root.app.borrow().snapshot_settings();
                if snapshot.ghost_layer_enabled {
                    if let Some(hwnd) = find_main_hwnd(root) {
                        if let Err(error) =
                            bento_nano_backend::ghost_layer::attach_selected_stack(hwnd)
                        {
                            tracing::warn!(
                                target: "bentodesk::ghost_layer",
                                %error,
                                "PowerResume ghost-layer reattach failed"
                            );
                        }
                    }
                    bento_nano_backend::ghost_layer::reposition_to_work_area();
                }
                match validate_settings_sources(&snapshot) {
                    Ok(sources) => {
                        if let Err(error) = rebuild_desktop_watcher(root, &sources) {
                            tracing::warn!(
                                target: "bentodesk::watcher",
                                %error,
                                "PowerResume watcher rebuild failed"
                            );
                        }
                    }
                    Err(error) => tracing::warn!(
                        target: "bentodesk::watcher",
                        %error,
                        "PowerResume watcher rebuild skipped for invalid Settings paths"
                    ),
                }
                tracing::info!(
                    target: "bentodesk::power",
                    "PowerResume recovery completed with saved delay and enablement"
                );
            }
        }
        if drained >= 8 {
            break;
        }
    }
    drained > 0
}

/// T-099 hibernation pass — runs once per paint cycle. Iterates every
/// registered slot looking for `pending_hibernate && (now - last_visible_change_ms ≤ HIBERNATE_GATE_MS)`,
/// and releases the swap chain backbuffer for each one. Main window slots
/// short-circuit (`pending_hibernate` is never set on them per
/// `WindowSlot::set_visible`).
///
/// The 500 ms gate avoids thrashing on rapid hide/show cycles. Below the
/// gate the backbuffer stays resident and the next show pays zero
/// recreation cost.
pub(super) fn flush_hibernation(root: &AppRoot, now_ms: u32) {
    // Brief borrow_mut for the iteration scope. The wndproc raw-pointer
    // paint path has just finished (we're between paints), so the
    // `&mut WindowSlot` reach through the registry is sound.
    let mut reg = match root.registry.try_borrow_mut() {
        Ok(r) => r,
        // Re-entrant during paint (shouldn't happen, but spec §11 says
        // degrade rather than panic).
        Err(_) => return,
    };
    for slot in reg.iter_mut() {
        if !slot.pending_hibernate.get() {
            continue;
        }
        let elapsed = now_ms.wrapping_sub(slot.last_visible_change_ms.get());
        if elapsed < HIBERNATE_GATE_MS {
            continue;
        }
        // §11 R5 — release backbuffer; `Renderer::ensure_swap_chain` rebuilds
        // it on the next WM_SHOWWINDOW(TRUE) → resize call.
        slot.renderer.release_swap_chain();
        slot.pending_hibernate.set(false);
    }
}
