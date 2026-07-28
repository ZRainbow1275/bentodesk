//! Native shell owner: `zone_motion`.

use super::*;

pub(super) fn proof_zone_id_label(id: Option<ZoneId>) -> String {
    match id {
        Some(zone_id) => zone_id.0.to_string(),
        None => "none".to_owned(),
    }
}

pub(super) fn proof_active_drag_label(app: &AppState) -> &'static str {
    if app.zone_drag.get().is_some() {
        "zone"
    } else if app.zone_resize.get().is_some() {
        "resize"
    } else if app.item_drag.borrow().is_some() {
        "item"
    } else if app.stack_tray_drag.get().is_some() {
        "stack_tray"
    } else {
        "none"
    }
}

pub(super) fn log_animation_proof_state(
    app: &AppState,
    phase: &str,
    now_ms: u32,
    x: Option<f32>,
    y: Option<f32>,
) {
    if !animation_proof_log_enabled() {
        return;
    }
    let hover_scheduler = app.hover_scheduler.get();
    let item_drag = {
        let drag = app.item_drag.borrow();
        drag.as_ref()
            .map(|candidate| format!("{}:{}", candidate.zone_id.0, candidate.item_id.0))
            .unwrap_or_else(|| "none".to_owned())
    };
    let item_hover = app.item_hover.get();
    let item_hover_active = item_hover.is_active(now_ms);
    let pill_animator_occupancy = app.pill_animator.borrow().occupancy();
    let (highlight_targets, highlight_pulses, highlight_auto_clear_ms) = {
        let overlay = app.highlight_overlay.borrow();
        (
            overlay.targets().len(),
            overlay.pulses().len(),
            overlay.auto_clear_remaining_ms().unwrap_or(0),
        )
    };
    let input = match (x, y) {
        (Some(px), Some(py)) => format!("{px:.1},{py:.1}"),
        _ => "none".to_owned(),
    };
    log_static(
        format!(
            "anim_state: phase={phase} now_ms={now_ms} input={input} active_drag={} zone_drag={} zone_resize={} item_drag={} stack_tray_drag={} hovered_zone={} selected_zone={} pill_anim_zone={} pill_anim_progress={:.3} pill_anim_morph={:.3} pill_anim_duration_ms={} pill_anim_expanding={} pill_animator_occupancy={} stack_bloom_anchor={} stack_bloom_progress={:.3} stack_bloom_leaving={} hover_scheduler_pending={} hover_scheduler_expanded={} item_hover_active={} item_hover={item_hover:?} highlight_targets={} highlight_pulses={} highlight_auto_clear_ms={} dirty={}\n",
            proof_active_drag_label(app),
            proof_zone_id_label(app.zone_drag.get().map(|(id, _, _)| id)),
            proof_zone_id_label(app.zone_resize.get().map(|(id, _, _)| id)),
            item_drag,
            app.stack_tray_drag.get().is_some(),
            proof_zone_id_label(app.hovered_zone.get()),
            proof_zone_id_label(app.selected_zone.get()),
            proof_zone_id_label(app.zone_pill_anim_zone.get()),
            app.zone_pill_anim_progress.get(),
            sampled_zone_pill_morph(app),
            app.zone_pill_anim_duration_ms.get(),
            app.zone_pill_anim_expanding.get(),
            pill_animator_occupancy,
            proof_zone_id_label(app.stack_bloom_anchor.get()),
            app.stack_bloom_progress.get(),
            app.stack_bloom_leaving.get(),
            hover_scheduler.is_pending(),
            proof_zone_id_label(hover_scheduler.expanded_zone()),
            item_hover_active,
            highlight_targets,
            highlight_pulses,
            highlight_auto_clear_ms,
            app.dirty.get()
        )
        .as_str(),
    );
}

pub(super) fn reset_pointer_drag_hover_channels(
    app: &AppState,
    dragged_zone: Option<ZoneId>,
    now_ms: u32,
) {
    if let Some(zone_id) = dragged_zone {
        let mut anim = app.pill_animator.borrow_mut();
        anim.cancel(zone_id, bento_nano_app::animator::AnimChannel::PillHover);
        anim.cancel(zone_id, bento_nano_app::animator::AnimChannel::PillPress);
        drop(anim);
        if app.zone_pill_anim_zone.get() == Some(zone_id) {
            app.zone_pill_anim_zone.set(None);
            app.zone_pill_anim_progress.set(1.0);
            app.zone_pill_anim_from_morph.set(0.0);
            app.zone_pill_anim_duration_ms
                .set(zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS);
            app.zone_pill_anim_expanding.set(false);
            app.zone_pill_anim_started_ms.set(now_ms);
        }
    }
    app.pill_pressed_zone.set(None);
    app.hovered_zone.set(None);
    app.set_panel_header_button_hover(None);
    app.set_settings_encryption_mode_hover(None);
    app.set_settings_close_hover(false);
    clear_stack_bloom_surface(app);
    app.highlight_overlay.borrow_mut().clear();
    let mut scheduler = app.hover_scheduler.get();
    scheduler.reset();
    app.hover_scheduler.set(scheduler);
    app.item_hover
        .set(bento_nano_app::business::item_card::ItemHoverState::new());
}

pub(super) fn clear_stack_tray_open_hover_state(app: &AppState) {
    app.hovered_zone.set(None);
    app.set_panel_header_button_hover(None);
    clear_stack_bloom_surface(app);
    app.pill_pressed_zone.set(None);
    let mut scheduler = app.hover_scheduler.get();
    scheduler.reset();
    app.hover_scheduler.set(scheduler);
    app.item_hover
        .set(bento_nano_app::business::item_card::ItemHoverState::new());
}

pub(super) fn tick_stack_bloom_animation(app: &AppState, now_ms: u32) -> bool {
    let Some(anchor_id) = app.stack_bloom_anchor.get() else {
        return false;
    };
    let elapsed = now_ms.wrapping_sub(app.stack_bloom_started_ms.get());
    let member_count = app
        .zones
        .stack_member_ids(anchor_id)
        .map(|members| members.len());
    let duration_ms = if app.stack_bloom_leaving.get() {
        member_count
            .map(stack_tray::stack_bloom_exit_duration_ms)
            .unwrap_or(stack_tray::BLOOM_EXIT_VISIBLE_DURATION_MS)
    } else {
        member_count
            .map(stack_tray::stack_bloom_reveal_duration_ms)
            .unwrap_or(stack_tray::BLOOM_REVEAL_DURATION_MS)
    };
    let progress = (elapsed as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let changed = (app.stack_bloom_progress.get() - progress).abs() > 0.001;
    app.stack_bloom_progress.set(progress);
    if changed || progress < 1.0 {
        log_animation_proof_state(app, "stack_bloom_tick", now_ms, None, None);
    }
    if app.stack_bloom_leaving.get() && progress >= 1.0 {
        clear_stack_bloom_surface(app);
        log_animation_proof_state(app, "stack_bloom_exit_done", now_ms, None, None);
        return true;
    }
    changed || progress < 1.0
}

#[inline]
pub(super) fn sampled_zone_pill_morph(app: &AppState) -> f32 {
    zone_pill_geometry::current_morph_progress(
        app.zone_pill_anim_from_morph.get(),
        app.zone_pill_anim_progress.get(),
        app.zone_pill_anim_expanding.get(),
    )
}

pub(super) fn begin_zone_pill_segment(
    app: &AppState,
    zone_id: ZoneId,
    from_morph: f32,
    expanding: bool,
    now_ms: u32,
) {
    // Reverse from the exact visible boundary. Geometry and expanded content
    // consume this same monotonic morph, so there is no second alpha timeline
    // to jump when the pointer changes direction mid-flight.
    let from = from_morph.clamp(0.0, 1.0);
    let target = if expanding { 1.0 } else { 0.0 };
    app.zone_pill_anim_zone.set(Some(zone_id));
    app.zone_pill_anim_from_morph.set(from);
    app.zone_pill_anim_duration_ms
        .set(zone_pill_geometry::pill_segment_duration_ms(from, target));
    app.zone_pill_anim_expanding.set(expanding);
    app.zone_pill_anim_started_ms.set(now_ms);
    app.zone_pill_anim_progress.set(0.0);
}

/// Start or reverse the capsule pill transition for the current hover target.
/// The segment records its painted start morph, so reversing an eased curve is
/// continuous instead of mirroring raw time (which is only correct for linear
/// interpolation). Stack anchors remain owned by the Bloom animation.
pub(super) fn update_zone_pill_hover(
    app: &AppState,
    hover_zone: Option<ZoneId>,
    now_ms: u32,
) -> bool {
    // Resolve the previous "expanded under hover" zone so we know what to
    // collapse if the pointer moved off it.
    let prev_zone = app.zone_pill_anim_zone.get();
    let prev_expanding = app.zone_pill_anim_expanding.get();

    // Treat stack anchors as non-pill so they don't steal the animation slot.
    let next_zone = hover_zone.and_then(|id| {
        let zone = app.zones.get(id)?;
        if zone.is_stack_anchor() {
            None
        } else {
            Some(id)
        }
    });

    // Steady state: pointer is on the same target as the current animation.
    if next_zone == prev_zone && prev_expanding {
        return false;
    }

    // Pointer moved to a different (or null) target — start a transition.
    let mut changed = false;
    if let Some(prev_id) = prev_zone {
        if prev_expanding && Some(prev_id) != next_zone {
            // Was expanding `prev_id`, but pointer left — continue from the
            // exact shape painted by the current eased segment.
            let current = sampled_zone_pill_morph(app);
            begin_zone_pill_segment(app, prev_id, current, false, now_ms);
            changed = true;
        }
    }

    if let Some(next_id) = next_zone {
        if Some(next_id) != prev_zone || !prev_expanding {
            // Either no prior animation, or it was a collapse. A same-zone
            // reversal samples the in-flight shape; a new zone starts at its
            // collapsed pill.
            let from = if prev_zone == Some(next_id) && !prev_expanding {
                sampled_zone_pill_morph(app)
            } else {
                0.0
            };
            begin_zone_pill_segment(app, next_id, from, true, now_ms);
            changed = true;
        }
    } else if prev_zone.is_some() && !prev_expanding && app.zone_pill_anim_progress.get() >= 1.0 {
        // Already collapsed — clear stale state so the renderer skips the
        // morph branch entirely.
        app.zone_pill_anim_zone.set(None);
        app.zone_pill_anim_progress.set(1.0);
        app.zone_pill_anim_from_morph.set(0.0);
        app.zone_pill_anim_duration_ms
            .set(zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS);
        changed = true;
    }

    changed
}

/// Wave G2 — advance the pill morph progress for whichever zone is currently
/// animating. Returns `true` while the animation is still in flight so the
/// shell keeps pumping frames; clears state when the collapse fully settles.
pub(super) fn tick_zone_pill_animation(app: &AppState, now_ms: u32) -> bool {
    let Some(_zone) = app.zone_pill_anim_zone.get() else {
        return false;
    };
    let elapsed = now_ms.wrapping_sub(app.zone_pill_anim_started_ms.get());
    let duration_ms = app.zone_pill_anim_duration_ms.get().max(1);
    let progress = (elapsed as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let prev = app.zone_pill_anim_progress.get();
    let changed = (prev - progress).abs() > 0.001;
    app.zone_pill_anim_progress.set(progress);
    if changed || progress < 1.0 {
        log_animation_proof_state(app, "zone_morph_tick", now_ms, None, None);
    }
    // Collapsing finished — drop the anim slot so the renderer falls back
    // to the steady pill chrome (no allocation per frame).
    if progress >= 1.0 && !app.zone_pill_anim_expanding.get() {
        app.zone_pill_anim_zone.set(None);
        app.zone_pill_anim_from_morph.set(0.0);
        app.zone_pill_anim_duration_ms
            .set(zone_pill_geometry::ZONE_PILL_ANIM_DURATION_MS);
        log_animation_proof_state(app, "hover_collapse_settled", now_ms, None, None);
    }
    changed || progress < 1.0
}

/// Structural hover is a display-mode capability, not a generic pointer
/// affordance. Micro hover tint/scale may still run in every mode, but only
/// `Hover` may arm a pill↔panel or stack-bloom state transition.
pub(super) fn zone_structurally_expands_on_hover(app: &AppState, zone_id: ZoneId) -> bool {
    app.zones
        .get(zone_id)
        .map(|zone| {
            matches!(
                app.effective_zone_display_mode(zone),
                bento_nano_app::ZoneDisplayMode::Hover
            )
        })
        .unwrap_or(false)
}

/// A3 (2026-05-29) — true when leaving `zone` should auto-collapse it. The
/// same policy gates enter and leave so Click/Always can never inherit one
/// half of the Hover state machine.
pub(super) fn zone_auto_collapses_on_leave(app: &AppState, zone_id: ZoneId) -> bool {
    zone_structurally_expands_on_hover(app, zone_id)
}

/// A3 — feed the hover-intent and grace-collapse scheduler from a hover-target
/// change. `hover_zone` is the zone the cursor is now over (already resolved
/// and stack-anchor-filtered upstream). The scheduler defers the structural
/// expand or collapse morph by the user-tunable `expand_delay_ms` and
/// `collapse_delay_ms` (M1d Settings sliders) so a transient pointer twitch no
/// longer instantly opens or closes a zone. Stack anchors keep their bespoke
/// chrome and never enter the pill scheduler.
pub(super) fn drive_hover_scheduler(app: &AppState, hover_zone: Option<ZoneId>, now_ms: u32) {
    // Treat stack anchors as non-pill so they don't engage the scheduler.
    let next_zone = hover_zone.and_then(|id| {
        let zone = app.zones.get(id)?;
        if zone.is_stack_anchor() || !zone_structurally_expands_on_hover(app, id) {
            None
        } else {
            Some(id)
        }
    });
    let expand_delay = app.expand_delay_ms.get().max(0) as u32;
    let collapse_delay = app.collapse_delay_ms.get().max(0) as u32;
    let mut scheduler = app.hover_scheduler.get();
    let expanded = scheduler.expanded_zone();

    match next_zone {
        Some(zone) => {
            // Cursor is over a zone. nano morphs a single zone at a time, so
            // a direct zone→zone hand-off is handled by the morph's single
            // slot (the new expand replaces the old expanded body — Tauri
            // treats position as instant). on_enter cancels any pending
            // collapse for this zone (re-enter aborts the grace) and arms the
            // hover-intent expand timer when it isn't already expanded.
            scheduler.on_enter(zone, now_ms, expand_delay);
        }
        None => {
            // Cursor moved to empty space. Arm the grace collapse for the
            // expanded zone (HOVER mode only); clears any pending expand.
            let auto = expanded
                .map(|z| zone_auto_collapses_on_leave(app, z))
                .unwrap_or(false);
            scheduler.on_leave(now_ms, collapse_delay, auto);
        }
    }
    app.hover_scheduler.set(scheduler);
    if next_zone.is_some() {
        log_animation_proof_state(app, "hover_enter_armed", now_ms, None, None);
    } else {
        log_animation_proof_state(app, "hover_leave_armed", now_ms, None, None);
    }
}

/// A3 — poll the scheduler once per frame and apply any due expand/collapse to
/// the Wave G2 morph state machine. Returns `true` when a structural change
/// fired (the caller should request a redraw). The morph itself is still
/// advanced by `tick_zone_pill_animation`; this just flips the target.
pub(super) fn poll_hover_scheduler(app: &AppState, now_ms: u32) -> bool {
    let mut scheduler = app.hover_scheduler.get();
    let action = scheduler.poll(now_ms);
    app.hover_scheduler.set(scheduler);
    match action {
        zone_pill_geometry::HoverAction::None => false,
        zone_pill_geometry::HoverAction::Expand(zone) => {
            let changed = update_zone_pill_hover(app, Some(zone), now_ms);
            log_animation_proof_state(app, "hover_expand_fired", now_ms, None, None);
            changed
        }
        zone_pill_geometry::HoverAction::Collapse(_zone) => {
            app.reset_zone_content_scroll();
            let changed = update_zone_pill_hover(app, None, now_ms);
            log_animation_proof_state(app, "hover_collapse_fired", now_ms, None, None);
            changed
        }
    }
}

pub(super) fn collapse_zone_from_header(app: &AppState, zone_id: ZoneId, now_ms: u32) -> bool {
    // Mouse-down selects the hit Zone before header-button dispatch. Without
    // clearing that selection, Hover/Click display modes immediately force the
    // expanded body visible again after the collapse animation settles.
    let body_was_visible = app
        .zones
        .get(zone_id)
        .is_some_and(|zone| app.zone_pill_body_visible(zone));
    let selection_changed = app.selected_zone.get() == Some(zone_id);
    if selection_changed {
        app.selected_zone.set(None);
    }
    let mut scheduler = app.hover_scheduler.get();
    scheduler.reset();
    app.hover_scheduler.set(scheduler);
    app.set_panel_header_button_hover(None);
    let scroll_changed = app.reset_zone_content_scroll();
    let morph_changed = if app.zone_pill_anim_zone.get().is_some() {
        update_zone_pill_hover(app, None, now_ms)
    } else if body_was_visible {
        // Click-selected zones can be fully open without a hover scheduler
        // marker. Closing still begins from the visible panel instead of
        // dropping directly to the capsule.
        begin_zone_pill_segment(app, zone_id, 1.0, false, now_ms);
        true
    } else {
        false
    };
    let hover_changed = update_pill_hover_animator(app, None, now_ms);
    selection_changed || scroll_changed || morph_changed || hover_changed
}

/// V-8 (2026-05-21) — drive the pill hover animator channel.
///
/// The Wave G2 `update_zone_pill_hover` above handles the **rect/radius
/// morph** between pill and expanded body (a structural transition). V-8
/// layers a separate hover micro-animation on top: a small ~4% scale-up
/// with brightened shadow/surface. This helper feeds the dedicated
/// `pill_animator` so the rect morph keeps its own state machine intact.
///
/// Stack anchors are excluded because they keep their bespoke chrome and
/// never paint via the V-8 pill path.
pub(super) fn update_pill_hover_animator(
    app: &AppState,
    hover_zone: Option<ZoneId>,
    now_ms: u32,
) -> bool {
    let prev = app.hovered_zone.get();
    let next = hover_zone.and_then(|id| {
        let zone = app.zones.get(id)?;
        if zone.is_stack_anchor() {
            None
        } else {
            Some(id)
        }
    });
    if prev == next {
        return false;
    }
    let mut anim = app.pill_animator.borrow_mut();
    if let Some(prev_id) = prev {
        let still_anchor = app
            .zones
            .get(prev_id)
            .map(|z| z.is_stack_anchor())
            .unwrap_or(true);
        if !still_anchor {
            anim.start_or_reverse(
                prev_id,
                bento_nano_app::animator::AnimChannel::PillHover,
                now_ms,
                bento_nano_app::animator::HOVER_OUT_DURATION_MS,
                0.0,
                bento_nano_app::animator::Easing::EaseOutCubic,
            );
        }
    }
    if let Some(next_id) = next {
        anim.start_or_reverse(
            next_id,
            bento_nano_app::animator::AnimChannel::PillHover,
            now_ms,
            bento_nano_app::animator::HOVER_IN_DURATION_MS,
            1.0,
            bento_nano_app::animator::Easing::EaseOutCubic,
        );
    }
    true
}

/// #2 step 5 (2026-06-02) — SINGLE hover-target-change entry point.
///
/// Previously the `WM_MOUSEMOVE` handler fired three channel drivers
/// (`update_pill_hover_animator`, `update_stack_bloom_hover`,
/// `drive_hover_scheduler`) back-to-back on the same `now_ms`, so a single
/// pointer move could arm overlapping animation channels. This consolidates
/// them into ONE function that branches by zone type so a given hover arms
/// exactly the channel that zone needs:
///
/// * **stack anchor** → the bloom only (its hover affordance is the petal fan,
///   never the pill→panel expand morph). The pill-hover + expand-scheduler
///   drivers already self-exclude anchors, so they are simply not engaged.
/// * **normal zone** → the pill-hover micro-animation + the expand scheduler;
///   the bloom is cleared (its `update` resolves a non-anchor to `None`).
///
/// The pill animator is sampled BEFORE `hovered_zone` is written so the helper
/// sees the old→new delta (preserving the V-8 ordering contract). The caller
/// owns the `request_redraw`.
pub(super) fn on_hover_target_changed(app: &AppState, hover_zone: Option<ZoneId>, now_ms: u32) {
    let is_anchor = hover_zone
        .and_then(|id| app.zones.get(id))
        .map(|z| z.is_stack_anchor())
        .unwrap_or(false);
    // V-8 ordering: sample the pill animator against the OLD hovered_zone
    // before we overwrite it. For a stack anchor this is a no-op (the helper
    // self-excludes anchors), so the pill channel never co-fires with bloom.
    update_pill_hover_animator(app, hover_zone, now_ms);
    app.hovered_zone.set(hover_zone);
    if is_anchor {
        let Some(anchor) = hover_zone else {
            return;
        };
        let mode = app
            .zones
            .get(anchor)
            .map(|zone| app.effective_zone_display_mode(zone))
            .unwrap_or(ZoneDisplayMode::Hover);
        let management_anchor = app
            .stack_tray
            .borrow()
            .as_ref()
            .filter(|state| state.is_management())
            .map(|state| state.anchor_zone_id);
        match mode {
            ZoneDisplayMode::Hover if management_anchor.is_none() => {
                update_stack_bloom_hover(app, hover_zone, now_ms);
            }
            ZoneDisplayMode::Hover | ZoneDisplayMode::Click | ZoneDisplayMode::Always => {
                // Click and Always must not create a structural surface from a
                // pointer-enter. Hover also stays clear while an explicit
                // management tray owns the stack surface.
                clear_stack_bloom_surface(app);
            }
        }
        drive_hover_scheduler(app, None, now_ms);
        log_animation_proof_state(app, "hover_changed", now_ms, None, None);
    } else {
        // Normal zone → expand scheduler only; clear the bloom anchor (the
        // helper resolves a non-anchor hover to `None`).
        update_stack_bloom_hover(app, hover_zone, now_ms);
        drive_hover_scheduler(app, hover_zone, now_ms);
        log_animation_proof_state(app, "hover_changed", now_ms, None, None);
    }
}

pub(super) fn update_main_zone_hover_for_point(
    app: &AppState,
    x: f32,
    y: f32,
    now_ms: u32,
) -> bool {
    let hover_zone = stack_aware_hover_zone_for_point(app, x, y);
    let mut changed = false;
    if app.hovered_zone.get() != hover_zone {
        on_hover_target_changed(app, hover_zone, now_ms);
        changed = true;
    }
    changed | update_stack_bloom_petal_hover(app, x, y, now_ms)
}

/// M3-A2 (2026-05-29) — drive the per-item hover scale ramp from a pointer
/// move over the Main overlay. Resolves the card under `(x, y)` via the SAME
/// `hit_test_zone_item` the drag-out path uses (so the hover ramp tracks the
/// base, unscaled V-13 hit-rect exactly), then re-targets `item_hover`. While
/// an item drag is in flight the hovered card is cleared — a card being
/// reordered shouldn't hover-pop under the floating ghost. Returns `true` when
/// the hovered target changed so the caller requests a redraw; the 150ms ramp
/// itself is then advanced by `tick_item_hover_animator` on the frame pump.
pub(super) fn update_item_hover_animator(app: &AppState, x: f32, y: f32) -> bool {
    let card = if app.item_drag.borrow().is_some() {
        None
    } else {
        item_hit_for_point(app, x, y)
    };
    // SAFETY: GetTickCount has no failure mode and is documented MT-safe.
    let now_ms = unsafe { GetTickCount() };
    let mut state = app.item_hover.get();
    let changed = state.on_hover(card, now_ms);
    app.item_hover.set(state);
    changed
}

pub(super) fn move_zone_live(app: &mut AppState, id: ZoneId, point: DispatchPoint) -> bool {
    if !app.zones.move_group_to(id, point.x, point.y) {
        return false;
    }
    app.mark_dirty();
    true
}

pub(super) fn zone_drag_pointer_offset(app: &AppState, id: ZoneId) -> Option<(i32, i32)> {
    let zone = app.zones.get(id)?;
    let (_, _, width, height) =
        bento_nano_app::zone_gesture_geometry::zone_drag_capsule_rect(&app.zones, zone);
    Some((width / 2, height / 2))
}

pub(super) fn resize_zone_live(app: &mut AppState, id: ZoneId, size: DispatchSize) -> bool {
    let Some(z) = app.zones.get_mut(id) else {
        return false;
    };
    let width = size.width.max(80);
    let height = size.height.max(60);
    if z.w == width && z.h == height {
        return false;
    }
    z.w = width;
    z.h = height;
    app.mark_dirty();
    true
}

/// M3-A2 — record a pointer-down on the item card at `(x, y)`, starting the
/// 80ms press ramp toward Tauri's `:active` `scale(0.97)`. No-op when the down
/// did not land on a card. Mirrors `start_pill_press_animator`.
pub(super) fn start_item_press_animator(
    app: &AppState,
    zone_id: ZoneId,
    item_id: ZoneItemId,
    now_ms: u32,
) {
    let mut state = app.item_hover.get();
    state.on_press((zone_id, item_id), now_ms);
    app.item_hover.set(state);
}

/// M3-A2 — release any in-flight item press on `WM_LBUTTONUP`, regardless of
/// where the up lands, so a drag-off still ramps the press back to rest.
/// Mirrors `release_pill_press_animator`. Returns `true` when a press was
/// actually releasing (caller requests a redraw to animate the ramp-back).
pub(super) fn release_item_press_animator(app: &AppState, now_ms: u32) -> bool {
    let mut state = app.item_hover.get();
    let changed = state.on_release(now_ms);
    app.item_hover.set(state);
    changed
}

/// M3-A2 — per-frame tick of the item hover/press ramps. Retires the leaving
/// (hover-out) card and a fully-released press so a stale entry can't pin the
/// pump. Returns `true` while any ramp is in flight so the shell keeps
/// requesting redraws (the 150ms hover / 80ms press transitions then animate).
pub(super) fn tick_item_hover_animator(app: &AppState, now_ms: u32) -> bool {
    let mut state = app.item_hover.get();
    let active = state.tick(now_ms);
    app.item_hover.set(state);
    active
}

/// V-8 — start the press-down half of the pill press animation. Called on
/// `WM_LBUTTONDOWN` after we've confirmed the click landed inside a pill
/// rect. The release half lives in `release_pill_press_animator`.
pub(super) fn start_pill_press_animator(app: &AppState, zone_id: ZoneId, now_ms: u32) {
    app.pill_pressed_zone.set(Some(zone_id));
    let mut anim = app.pill_animator.borrow_mut();
    anim.start_or_reverse(
        zone_id,
        bento_nano_app::animator::AnimChannel::PillPress,
        now_ms,
        bento_nano_app::animator::PRESS_DOWN_DURATION_MS,
        1.0,
        bento_nano_app::animator::Easing::EaseOutCubic,
    );
}

/// V-8 — release the in-flight pill press regardless of where the
/// `WM_LBUTTONUP` lands. Called from the global mouse-up handler so a
/// drag-off still tidies the press visual.
pub(super) fn release_pill_press_animator(app: &AppState, now_ms: u32) -> bool {
    let Some(zone_id) = app.pill_pressed_zone.replace(None) else {
        return false;
    };
    let mut anim = app.pill_animator.borrow_mut();
    anim.start_or_reverse(
        zone_id,
        bento_nano_app::animator::AnimChannel::PillPress,
        now_ms,
        bento_nano_app::animator::PRESS_UP_DURATION_MS,
        0.0,
        bento_nano_app::animator::Easing::EaseOutCubic,
    );
    true
}

/// V-8 per-frame tick of the pill hover/press animator. Drops fully-decayed
/// entries and returns `true` only while a sampled visual transition is still
/// in flight. `StatusDotPulse` helpers are dormant until a paint consumer
/// samples them, so they must not keep the main window repainting by themselves.
pub(super) fn tick_pill_animator(app: &AppState, now_ms: u32) -> bool {
    let mut anim = app.pill_animator.borrow_mut();
    anim.tick(now_ms)
}
