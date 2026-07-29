//! Native shell owner: `stack_interaction`.

use super::*;

pub(super) fn stack_bloom_hit_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<(ZoneId, ZoneId)> {
    if app.settings_open.get() || app.about_open.get() {
        return None;
    }
    // #5 drag stability (2026-06-08) — a normal zone/item drag owns the
    // pointer until mouse-up. Do not let a stale bloom hit consume that release
    // and open/preview a stack before `handle_lbutton_up` clears `zone_drag`.
    if app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
    {
        return None;
    }
    // #4 / R1 (2026-06-02) — mirror the render-side bloom gate so a click only
    // hits a petal on a frame where the bloom is actually painted: the tray
    // must be closed and no member focused/selected (mutually exclusive
    // surfaces — never a hit behind the tray or a focused-member panel).
    if app
        .stack_tray
        .borrow()
        .as_ref()
        .is_some_and(StackTrayState::is_management)
        || app.selected_zone.get().is_some()
    {
        return None;
    }
    // The renderer uses `stack_bloom_anchor` as the single structural state.
    // Do not synthesize invisible petal hits from `hovered_zone`: a pointer
    // drop explicitly arms this state only after the stack relation exists.
    let anchor_id = app.stack_bloom_anchor.get()?;
    let anchor = app.zones.get(anchor_id)?;
    let members = app.zones.stack_member_ids(anchor.id)?;
    // Tauri round-13 `no-auto-open`: Bloom petals spring out from the
    // capsule centre. During the first part of that animation their visual
    // hit rects overlap the still-clickable capsule. Treating the animated
    // petal as the top hit arms the 150 ms preview timer even though the
    // pointer only entered (and remained on) the capsule, which can both
    // auto-open a member and steal the capsule's collapse click. The capsule
    // is the stable interaction surface, so it owns any transient overlap;
    // settled petals remain fully interactive outside this rect.
    let capsule = zone_pill_geometry::stack_capsule_layout_for_zone(anchor, members.len()).rect;
    if x >= capsule.x && x <= capsule.right() && y >= capsule.y && y <= capsule.bottom() {
        return None;
    }
    let petal_index =
        if app.stack_bloom_leaving.get() && app.stack_bloom_anchor.get() == Some(anchor.id) {
            stack_tray::stack_bloom_exit_hit_test_at(
                app.viewport,
                anchor,
                members.len(),
                app.stack_bloom_progress.get(),
                x,
                y,
            )?
        } else {
            let reveal_progress = stack_bloom_reveal_progress_for_anchor(app, anchor.id);
            stack_tray::stack_bloom_hit_test_at(
                app.viewport,
                anchor,
                members.len(),
                reveal_progress,
                x,
                y,
            )?
        };
    let member_index = stack_tray::stack_bloom_member_index_for_petal(members.len(), petal_index)?;
    members
        .get(member_index)
        .copied()
        .map(|member_id| (anchor.id, member_id))
}

pub(super) fn stack_bloom_preview_hit_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<(ZoneId, ZoneId, bentodesk_style::Rect)> {
    let state = app.stack_tray.borrow().clone()?;
    if !state.is_bloom_preview() {
        return None;
    }
    let anchor = app.zones.get(state.anchor_zone_id)?;
    let members = app.zones.stack_member_ids(anchor.id)?;
    let member_index = members
        .iter()
        .position(|member_id| *member_id == state.selected_member_id)?;
    let member = app.zones.get(state.selected_member_id)?;
    let petals = stack_tray::stack_bloom_petal_rects(app.viewport, anchor, members.len());
    let petal = petals.get(member_index).copied()?;
    let preview = stack_tray::focused_bloom_preview_rect(app.viewport, petal, &petals, member);
    (x >= preview.x && x <= preview.right() && y >= preview.y && y <= preview.bottom())
        .then_some((anchor.id, member.id, preview))
}

pub(super) fn stack_bloom_preview_item_hit_for_point(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<(ZoneId, ZoneId, ZoneItemId)> {
    let (anchor, member, preview) = stack_bloom_preview_hit_for_point(app, x, y)?;
    let zone = app.zones.get(member)?;
    let search_active = app.zone_search_target.get() == Some(member);
    let search_state = app.search_bar.borrow();
    let query = search_state.query.as_str();
    let mut flow_slot = 0;
    for item in &zone.items {
        if search_active && !search_bar::zone_item_matches_query(item.name.as_ref(), query) {
            continue;
        }
        let (rect, next_slot) = highlight_overlay::item_card_rect_for_flow_slot_in_panel(
            zone,
            preview,
            flow_slot,
            item.is_wide,
            if search_active {
                search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX
            } else {
                0.0
            },
        );
        flow_slot = next_slot;
        if rect.width > 0.0
            && rect.height > 0.0
            && x >= rect.x
            && x < rect.right()
            && y >= rect.y
            && y < rect.bottom()
        {
            return Some((anchor, member, item.id));
        }
    }
    None
}

pub(super) fn item_hit_for_point(app: &AppState, x: f32, y: f32) -> Option<(ZoneId, ZoneItemId)> {
    stack_bloom_preview_item_hit_for_point(app, x, y)
        .map(|(_, member, item)| (member, item))
        .or_else(|| ui::hit_test_zone_item(app, x, y).map(|(zone, item, _)| (zone, item)))
}

pub(super) fn item_drag_target_zone_for_point(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    stack_bloom_preview_hit_for_point(app, x, y)
        .map(|(_, member, _)| member)
        .or_else(|| ui::hit_test_zone(app, x, y))
}

pub(super) fn item_grid_position_for_drag_point(
    app: &AppState,
    zone_id: ZoneId,
    x: f32,
    y: f32,
) -> Option<(i32, i32)> {
    if let Some((_, member, preview)) = stack_bloom_preview_hit_for_point(app, x, y)
        && member == zone_id
    {
        let zone = app.zones.get(member)?;
        return highlight_overlay::item_grid_position_for_panel(
            preview,
            zone.grid_columns,
            x,
            y,
            if app.zone_search_target.get() == Some(member) {
                search_bar::ZONE_INLINE_ITEM_OFFSET_Y_PX
            } else {
                0.0
            },
        );
    }
    ui::item_grid_position_for_point(app, zone_id, x, y)
}

pub(super) fn item_open_command_for_double_click(
    app: &AppState,
    x: f32,
    y: f32,
) -> Option<Command> {
    let (zone_id, item_id) = item_hit_for_point(app, x, y)?;
    let item = app
        .zones
        .get(zone_id)?
        .items
        .iter()
        .find(|item| item.id == item_id)?;
    (!item.file_missing).then_some(Command::OpenItemFile(
        zone_id,
        bentodesk_app::ItemId(item_id.0),
    ))
}

pub(super) fn stack_bloom_hover_anchor_for_point(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    stack_bloom_hit_for_point(app, x, y)
        .map(|(anchor, _)| anchor)
        .or_else(|| stack_bloom_preview_hit_for_point(app, x, y).map(|(anchor, _, _)| anchor))
}

/// Resolve the visual top layer first. Bloom petals and the focused preview are
/// painted above ordinary zones, so an unrelated zone geometrically underneath
/// them must not steal hover and collapse the Bloom while the cursor is visibly
/// on a petal.
pub(super) fn stack_aware_hover_zone_for_point(app: &AppState, x: f32, y: f32) -> Option<ZoneId> {
    stack_bloom_hover_anchor_for_point(app, x, y).or_else(|| ui::hit_test_zone(app, x, y))
}

pub(super) fn stack_bloom_reveal_progress_for_anchor(app: &AppState, anchor: ZoneId) -> f32 {
    if app.stack_bloom_anchor.get() == Some(anchor) {
        app.stack_bloom_progress.get()
    } else {
        1.0
    }
}

pub(super) fn reset_stack_bloom_interaction(app: &AppState) {
    app.stack_bloom_interaction
        .set(bentodesk_app::state::StackBloomInteractionState::default());
}

pub(super) fn start_stack_bloom_exit(app: &AppState, now_ms: u32) -> bool {
    if app.stack_bloom_anchor.get().is_none() || app.stack_bloom_leaving.get() {
        return false;
    }
    if app
        .stack_tray
        .borrow()
        .as_ref()
        .is_some_and(StackTrayState::is_bloom_preview)
    {
        app.stack_tray.borrow_mut().take();
    }
    reset_stack_bloom_interaction(app);
    app.stack_bloom_leaving.set(true);
    app.stack_bloom_started_ms.set(now_ms);
    app.stack_bloom_progress.set(0.0);
    true
}

pub(super) fn clear_stack_bloom_surface(app: &AppState) {
    if app
        .stack_tray
        .borrow()
        .as_ref()
        .is_some_and(StackTrayState::is_bloom_preview)
    {
        app.stack_tray.borrow_mut().take();
    }
    app.stack_bloom_anchor.set(None);
    app.stack_bloom_leaving.set(false);
    app.stack_bloom_progress.set(1.0);
    reset_stack_bloom_interaction(app);
}

pub(super) fn update_stack_bloom_hover(
    app: &AppState,
    hover_zone: Option<ZoneId>,
    now_ms: u32,
) -> bool {
    let next_anchor = hover_zone.and_then(|zone_id| app.zones.stack_anchor_for(zone_id));
    let current_anchor = app.stack_bloom_anchor.get();
    if let Some(anchor) = next_anchor {
        if current_anchor == Some(anchor) && !app.stack_bloom_leaving.get() {
            let mut interaction = app.stack_bloom_interaction.get();
            let changed = interaction.leave_started_ms.take().is_some();
            app.stack_bloom_interaction.set(interaction);
            return changed;
        }
        if current_anchor != Some(anchor) {
            reset_stack_bloom_interaction(app);
        } else {
            let mut interaction = app.stack_bloom_interaction.get();
            interaction.leave_started_ms = None;
            app.stack_bloom_interaction.set(interaction);
        }
        app.stack_bloom_anchor.set(Some(anchor));
        app.stack_bloom_leaving.set(false);
        app.stack_bloom_started_ms.set(now_ms);
        app.stack_bloom_progress.set(0.0);
        return true;
    }
    if current_anchor.is_some() && !app.stack_bloom_leaving.get() {
        if hover_zone.is_none() {
            let mut interaction = app.stack_bloom_interaction.get();
            if interaction.leave_started_ms.is_none() {
                interaction.leave_started_ms = Some(now_ms);
                app.stack_bloom_interaction.set(interaction);
                return true;
            }
            return false;
        }
        // An unrelated zone is a real target, not a one-pixel family gap. Match
        // Tauri v9 and yield to it immediately.
        return start_stack_bloom_exit(app, now_ms);
    }
    false
}

/// Track petal-level intent independently from the stack anchor hover. Moving
/// between two petals leaves `hovered_zone == anchor`, so relying only on the
/// zone-level transition would never retarget the focused preview.
pub(super) fn update_stack_bloom_petal_hover(app: &AppState, x: f32, y: f32, now_ms: u32) -> bool {
    let petal_hit = (!normal_pointer_drag_active(app)
        && !app.stack_bloom_leaving.get()
        && app.stack_bloom_anchor.get().is_some())
    .then(|| stack_bloom_hit_for_point(app, x, y))
    .flatten()
    .filter(|(anchor, _)| app.stack_bloom_anchor.get() == Some(*anchor));

    let mut interaction = app.stack_bloom_interaction.get();
    let mut changed = false;
    match petal_hit {
        Some((anchor, member)) => {
            let reentered = interaction.active_member_leave_started_ms.take().is_some();
            if interaction.active_member != Some(member) || reentered {
                interaction.active_member = Some(member);
                interaction.active_member_started_ms = now_ms;
                interaction.hover_preview_opened = false;
                changed = true;
            }
            if interaction.preview_sticky {
                let should_switch = app.stack_tray.borrow().as_ref().is_some_and(|state| {
                    state.is_bloom_preview()
                        && (state.anchor_zone_id != anchor || state.selected_member_id != member)
                });
                if should_switch {
                    app.stack_tray
                        .borrow_mut()
                        .replace(StackTrayState::bloom_preview(anchor, member));
                    interaction.hover_preview_opened = true;
                    changed = true;
                }
            }
        }
        None => {
            if interaction.active_member.is_some()
                && interaction.active_member_leave_started_ms.is_none()
            {
                interaction.active_member_leave_started_ms = Some(now_ms);
                // Petal leave cancels a not-yet-fired preview timer immediately;
                // the active ring alone gets the short visual grace.
                interaction.hover_preview_opened = true;
                changed = true;
            }
        }
    }
    app.stack_bloom_interaction.set(interaction);
    if changed && drag_proof_log_enabled() {
        log_static(
            format!(
                "stack: PetalHover point={x:.1},{y:.1} now_ms={now_ms} active={} started_ms={} leave_started={} preview_opened={} sticky={}\n",
                proof_zone_id_label(interaction.active_member),
                interaction.active_member_started_ms,
                interaction
                    .active_member_leave_started_ms
                    .map(|started| started.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                interaction.hover_preview_opened,
                interaction.preview_sticky,
            )
            .as_str(),
        );
    }
    changed
}

/// Advance the two short Bloom interaction deadlines from the existing hover
/// frame timer. Returns true only when visible state changed.
pub(super) fn poll_stack_bloom_interaction(app: &AppState, now_ms: u32) -> bool {
    if app.stack_bloom_anchor.get().is_none() || app.stack_bloom_leaving.get() {
        return false;
    }
    let mut interaction = app.stack_bloom_interaction.get();
    if interaction
        .leave_started_ms
        .is_some_and(|started| now_ms.wrapping_sub(started) >= stack_tray::BLOOM_LEAVE_GRACE_MS)
    {
        return start_stack_bloom_exit(app, now_ms);
    }

    let mut changed = false;
    if interaction
        .active_member_leave_started_ms
        .is_some_and(|started| now_ms.wrapping_sub(started) >= stack_tray::BLOOM_LEAVE_GRACE_MS)
    {
        interaction.active_member = None;
        interaction.active_member_leave_started_ms = None;
        changed = true;
    }

    if let (Some(anchor), Some(member)) = (app.stack_bloom_anchor.get(), interaction.active_member)
        && interaction.active_member_leave_started_ms.is_none()
        && !interaction.hover_preview_opened
        && now_ms.wrapping_sub(interaction.active_member_started_ms)
            >= stack_tray::BLOOM_PREVIEW_HOVER_INTENT_MS
    {
        let valid = app
            .zones
            .stack_member_ids(anchor)
            .is_some_and(|members| members.contains(&member));
        if valid {
            let already_open = app.stack_tray.borrow().as_ref().is_some_and(|state| {
                state.is_bloom_preview()
                    && state.anchor_zone_id == anchor
                    && state.selected_member_id == member
            });
            if !already_open {
                app.stack_tray
                    .borrow_mut()
                    .replace(StackTrayState::bloom_preview(anchor, member));
                log_static(
                    format!(
                        "stack: HoverPreviewStackMember anchor={} member={} intent_ms={}\n",
                        anchor.0,
                        member.0,
                        stack_tray::BLOOM_PREVIEW_HOVER_INTENT_MS
                    )
                    .as_str(),
                );
                changed = true;
            }
            interaction.hover_preview_opened = true;
            interaction.preview_sticky = false;
        }
    }
    app.stack_bloom_interaction.set(interaction);
    changed
}

/// Keep a moved free Zone collapsed under the unchanged release point. Tauri
/// collapses an expanded panel at drag start and does not immediately expand
/// that same free Zone again on mouse-up.
pub(super) fn hold_free_zone_drag_result_collapsed_until_reentry(
    app: &AppState,
    hover_zone: ZoneId,
    clear_selection: bool,
) {
    if clear_selection {
        app.selected_zone.set(None);
    }
    app.hovered_zone.set(Some(hover_zone));
    clear_stack_bloom_surface(app);
    let mut scheduler = app.hover_scheduler.get();
    scheduler.reset();
    app.hover_scheduler.set(scheduler);
}

/// Reveal the stack surface under the release pointer after its model relation
/// exists. The Tauri `StackWrapper` is immediately `:hover` after drop, so its
/// petals bloom at once while the focused member preview remains closed until
/// the existing 150 ms petal-hover intent fires.
pub(super) fn reveal_stack_at_drop_pointer(app: &AppState, anchor: ZoneId, now_ms: u32) {
    app.selected_zone.set(None);
    // Force the consolidated hover driver to observe a fresh target even when
    // the drag settled over an anchor that was hovered before mouse-down.
    app.hovered_zone.set(None);
    on_hover_target_changed(app, Some(anchor), now_ms);
}

/// Apply the Tauri StackCapsule click contract for the anchor's effective
/// display mode. Hover mode toggles Bloom as a motionless-drop fallback; Click
/// mode toggles the management tray; Always mode is already pinned and is a
/// no-op.
pub(super) fn toggle_stack_bloom_from_capsule_click(
    app: &AppState,
    anchor: ZoneId,
    now_ms: u32,
) -> bool {
    let Some(zone) = app.zones.get(anchor) else {
        return false;
    };
    app.selected_zone.set(None);
    app.hovered_zone.set(Some(anchor));
    match app.effective_zone_display_mode(zone) {
        ZoneDisplayMode::Hover => {
            if app.stack_bloom_anchor.get() == Some(anchor) && !app.stack_bloom_leaving.get() {
                start_stack_bloom_exit(app, now_ms)
            } else {
                update_stack_bloom_hover(app, Some(anchor), now_ms)
            }
        }
        ZoneDisplayMode::Click => {
            clear_stack_bloom_surface(app);
            let management_open = app
                .stack_tray
                .borrow()
                .as_ref()
                .is_some_and(|state| state.is_management() && state.anchor_zone_id == anchor);
            if management_open {
                app.stack_tray.borrow_mut().take();
            } else {
                app.stack_tray
                    .borrow_mut()
                    .replace(StackTrayState::new(anchor, anchor));
            }
            true
        }
        ZoneDisplayMode::Always => false,
    }
}

pub(super) fn normal_pointer_drag_active(app: &AppState) -> bool {
    app.zone_drag.get().is_some()
        || app.zone_resize.get().is_some()
        || app.item_drag.borrow().is_some()
}

pub(super) fn zone_accepts_click_expand(
    app: &AppState,
    zone_id: ZoneId,
    body_was_visible: bool,
) -> bool {
    !body_was_visible
        && app.zones.get(zone_id).is_some_and(|zone| {
            !zone.is_stack_anchor()
                && app.effective_zone_display_mode(zone) == ZoneDisplayMode::Click
        })
}
