//! Native shell owner: `timeline_storage`.

use super::*;

pub(super) fn capture_current_timeline_snapshot(root: &AppRoot, name: &str) -> DesktopSnapshot {
    let app = root.app.borrow();
    let width = app.viewport.width.max(1.0).round() as u32;
    let height = app.viewport.height.max(1.0).round() as u32;
    DesktopSnapshot {
        id: timeline::new_checkpoint_id(),
        name: name.to_owned(),
        resolution: Resolution { width, height },
        dpi: 1.0,
        zones: bento_zones_from_app(&app),
        captured_at: SmolStr::new(bentodesk_backend::time::now_rfc3339()),
    }
}

pub(super) fn save_timeline_checkpoint(
    root: &AppRoot,
    checkpoint_id: Option<SmolStr>,
    label: Option<SmolStr>,
) -> Result<Checkpoint, TimelineError> {
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    let checkpoint = if let Some(checkpoint_id) = checkpoint_id {
        if checkpoint_id.trim().is_empty() {
            return Err(TimelineError::EmptyCheckpointId);
        }
        root.timeline_buffer
            .borrow_mut()
            .pin(&store, checkpoint_id.as_str())
            .ok_or_else(|| TimelineError::CheckpointNotFound(checkpoint_id.clone()))?
    } else {
        let summary = label
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| localized_current("手动保存", "manual save"));
        let checkpoint = Checkpoint {
            id: timeline::new_checkpoint_id(),
            snapshot: capture_current_timeline_snapshot(root, summary.as_str()),
            delta: DeltaSummary::default(),
            delta_summary: summary.to_string(),
            trigger: SmolStr::new_static("manual"),
            coalesce_key: None,
            pinned: true,
        };
        root.timeline_buffer
            .borrow_mut()
            .push_pinned(&store, checkpoint.clone());
        checkpoint
    };
    sync_timeline_panel_from_buffer(root)?;
    set_timeline_status(
        root,
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "已保存当前布局记录".to_owned()
            } else {
                format!("Saved checkpoint {}", checkpoint.id)
            },
        ),
    );
    Ok(checkpoint)
}

pub(super) fn save_pre_restore_checkpoint(root: &AppRoot, store: &CheckpointStore) {
    let summary = localized_current("恢复前布局", "Pre-restore layout");
    let checkpoint = Checkpoint {
        id: timeline::new_checkpoint_id(),
        snapshot: capture_current_timeline_snapshot(root, summary.as_str()),
        delta: DeltaSummary::default(),
        delta_summary: summary.to_string(),
        trigger: SmolStr::new_static("pre_restore"),
        coalesce_key: None,
        pinned: false,
    };
    root.timeline_buffer
        .borrow_mut()
        .push_auto(store, checkpoint);
}

pub(super) fn push_coalesced_auto_timeline_checkpoint(
    root: &AppRoot,
    snapshot: DesktopSnapshot,
    delta: DeltaSummary,
    delta_summary: String,
    trigger: &str,
    coalesce_key: SmolStr,
    mode: AutoCoalesceMode,
) -> Result<Checkpoint, TimelineError> {
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    let checkpoint = Checkpoint {
        id: timeline::new_checkpoint_id(),
        snapshot,
        delta,
        delta_summary,
        trigger: SmolStr::new(trigger),
        coalesce_key: Some(coalesce_key.clone()),
        pinned: false,
    };
    let checkpoint = root.timeline_buffer.borrow_mut().push_auto_coalesced(
        &store,
        checkpoint,
        coalesce_key,
        mode,
    );
    sync_timeline_panel_from_buffer(root)?;
    Ok(checkpoint)
}

pub(super) fn report_timeline_coalesce_key(
    report: &ExecutionReport,
    phase: &str,
    trigger: &str,
) -> SmolStr {
    let key = report
        .checkpoint_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(trigger);
    SmolStr::new(format!("rules:{phase}:{trigger}:{key}"))
}

pub(super) fn sorted_zone_ids_key(ids: &[ZoneId]) -> String {
    let mut values = ids.iter().map(|id| id.0).collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn record_coalesced_mutation_timeline_pair(
    root: &AppRoot,
    before_snapshot: DesktopSnapshot,
    pre_trigger: &str,
    post_trigger: &str,
    coalesce_scope: &str,
    status: SmolStr,
) {
    let after_snapshot = capture_current_timeline_snapshot(root, status.as_str());
    let current_delta = timeline::compute_delta(Some(&before_snapshot), &after_snapshot.zones);
    if current_delta == DeltaSummary::default() {
        return;
    }
    let pre_key = SmolStr::new(format!("mutation:pre:{post_trigger}:{coalesce_scope}"));
    let pre_checkpoint = match push_coalesced_auto_timeline_checkpoint(
        root,
        before_snapshot,
        DeltaSummary::default(),
        format!("before {post_trigger}"),
        pre_trigger,
        pre_key,
        AutoCoalesceMode::KeepFirst,
    ) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::timeline",
                error = %error,
                trigger = post_trigger,
                "Mutation pre-checkpoint failed"
            );
            return;
        }
    };
    let coalesced_delta =
        timeline::compute_delta(Some(&pre_checkpoint.snapshot), &after_snapshot.zones);
    let post_key = SmolStr::new(format!("mutation:post:{post_trigger}:{coalesce_scope}"));
    if let Err(error) = push_coalesced_auto_timeline_checkpoint(
        root,
        after_snapshot,
        coalesced_delta.clone(),
        coalesced_delta.human(),
        post_trigger,
        post_key,
        AutoCoalesceMode::ReplaceLatest,
    ) {
        tracing::warn!(
            target: "bentodesk::timeline",
            error = %error,
            trigger = post_trigger,
            "Mutation post-checkpoint failed"
        );
    } else {
        set_timeline_status(root, status);
    }
}

pub(super) fn record_rule_execution_timeline_pair(
    root: &AppRoot,
    before_snapshot: DesktopSnapshot,
    report: &ExecutionReport,
) {
    if report.matched == 0 || report.actions_taken.is_empty() {
        return;
    }
    let after_snapshot = capture_current_timeline_snapshot(root, "after rule run");
    let current_delta = timeline::compute_delta(Some(&before_snapshot), &after_snapshot.zones);
    if current_delta == DeltaSummary::default() {
        return;
    }
    let rule_trigger = report.checkpoint_trigger.as_str();
    let before_summary = format!("before {rule_trigger}");
    let pre_key = report_timeline_coalesce_key(report, "pre", "rule_pre_apply");
    let pre_checkpoint = match push_coalesced_auto_timeline_checkpoint(
        root,
        before_snapshot,
        DeltaSummary::default(),
        before_summary,
        "rule_pre_apply",
        pre_key,
        AutoCoalesceMode::KeepFirst,
    ) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            tracing::warn!(
                target: "bentodesk::timeline",
                error = %error,
                "Rule execution pre-checkpoint failed"
            );
            return;
        }
    };
    let post_trigger = report.checkpoint_trigger.as_str();
    let coalesced_delta =
        timeline::compute_delta(Some(&pre_checkpoint.snapshot), &after_snapshot.zones);
    let post_summary = coalesced_delta.human();
    let post_key = report_timeline_coalesce_key(report, "post", post_trigger);
    if let Err(error) = push_coalesced_auto_timeline_checkpoint(
        root,
        after_snapshot,
        coalesced_delta,
        post_summary,
        post_trigger,
        post_key,
        AutoCoalesceMode::ReplaceLatest,
    ) {
        tracing::warn!(
            target: "bentodesk::timeline",
            error = %error,
            "Rule execution post-checkpoint failed"
        );
    } else {
        set_timeline_status(
            root,
            SmolStr::new_static(context_menu_text(
                "已记录规则运行前后的布局",
                "Rule execution checkpointed",
            )),
        );
    }
}

pub(super) fn restore_timeline_checkpoint(
    root: &AppRoot,
    checkpoint_id: &str,
) -> Result<SmolStr, TimelineError> {
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    let target = load_timeline_checkpoint(root, checkpoint_id)?;
    save_pre_restore_checkpoint(root, &store);
    apply_timeline_checkpoint(root, &target);
    root.timeline_buffer.borrow_mut().seek(checkpoint_id);
    sync_timeline_panel_from_buffer(root)?;
    set_timeline_status(
        root,
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "已恢复所选布局记录".to_owned()
            } else {
                format!("Restored checkpoint {}", target.id)
            },
        ),
    );
    Ok(target.id)
}

pub(super) fn undo_timeline_checkpoint(root: &AppRoot) -> Result<Option<SmolStr>, TimelineError> {
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    let target = root.timeline_buffer.borrow_mut().step_back();
    let Some(target) = target else {
        set_timeline_status(
            root,
            SmolStr::new_static(context_menu_text(
                "没有更早的布局记录可撤销",
                "No earlier checkpoint to undo",
            )),
        );
        return Ok(None);
    };
    apply_timeline_checkpoint(root, &target);
    sync_timeline_panel_from_buffer(root)?;
    set_timeline_status(
        root,
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "已撤销到上一条布局记录".to_owned()
            } else {
                format!("Undo restored {}", target.id)
            },
        ),
    );
    Ok(Some(target.id))
}

pub(super) fn redo_timeline_checkpoint(root: &AppRoot) -> Result<Option<SmolStr>, TimelineError> {
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    let target = root.timeline_buffer.borrow_mut().step_forward();
    let Some(target) = target else {
        set_timeline_status(
            root,
            SmolStr::new_static(context_menu_text(
                "没有更晚的布局记录可重做",
                "No later checkpoint to redo",
            )),
        );
        return Ok(None);
    };
    apply_timeline_checkpoint(root, &target);
    sync_timeline_panel_from_buffer(root)?;
    set_timeline_status(
        root,
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "已重做到下一条布局记录".to_owned()
            } else {
                format!("Redo restored {}", target.id)
            },
        ),
    );
    Ok(Some(target.id))
}

pub(super) fn delete_timeline_checkpoint(
    root: &AppRoot,
    checkpoint_id: &str,
) -> Result<(), TimelineError> {
    if checkpoint_id.trim().is_empty() {
        return Err(TimelineError::EmptyCheckpointId);
    }
    let store = current_timeline_store(root)?;
    ensure_timeline_loaded(root, &store);
    if !root
        .timeline_buffer
        .borrow_mut()
        .remove(&store, checkpoint_id)
    {
        return Err(TimelineError::CheckpointNotFound(SmolStr::new(
            checkpoint_id,
        )));
    }
    sync_timeline_panel_from_buffer(root)?;
    set_timeline_status(
        root,
        SmolStr::new(
            if bentodesk_style::current_locale_is(&bentodesk_style::ZH_CN) {
                "已删除布局记录".to_owned()
            } else {
                format!("Deleted checkpoint {checkpoint_id}")
            },
        ),
    );
    Ok(())
}

pub(super) fn apply_timeline_checkpoint(root: &AppRoot, checkpoint: &Checkpoint) {
    let mut app = root.app.borrow_mut();
    app.zones = zone_list_from_bento_zones(&checkpoint.snapshot.zones, app.viewport);
    bump_next_zone_id_from_zones(&app);
    app.mark_dirty();
}

pub(super) fn bump_next_zone_id_from_zones(app: &AppState) {
    let next = app
        .zones
        .iter()
        .map(|zone| zone.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
        .max(1);
    if app.next_zone_id.get() < next {
        app.next_zone_id.set(next);
    }
}

pub(super) fn bento_zones_from_app(app: &AppState) -> Vec<BentoZone> {
    let width = app.viewport.width.max(1.0);
    let height = app.viewport.height.max(1.0);
    let now = bentodesk_backend::time::now_rfc3339();
    app.zones
        .iter()
        .enumerate()
        .map(|(index, zone)| {
            let zone_id = SmolStr::new(zone.id.0.to_string());
            let items = zone
                .items
                .iter()
                .map(|item| bento_item_from_zone_item(zone_id.clone(), item, &now))
                .collect();
            BentoZone {
                id: zone_id,
                name: zone.title.to_string(),
                icon: SmolStr::new(zone.icon.as_ref()),
                position: RelativePosition {
                    x_percent: rules_percent(zone.x, width),
                    y_percent: rules_percent(zone.y, height),
                },
                expanded_size: RelativeSize {
                    w_percent: rules_percent(zone.w, width),
                    h_percent: rules_percent(zone.h, height),
                },
                items,
                accent_color: zone.accent_color.as_deref().map(SmolStr::new),
                sort_order: index as i32,
                auto_group: None,
                grid_columns: zone.grid_columns,
                created_at: SmolStr::new(now.as_str()),
                updated_at: SmolStr::new(now.as_str()),
                capsule_size: SmolStr::new(zone.capsule_size.as_ref()),
                capsule_shape: SmolStr::new(zone.capsule_shape.as_ref()),
                locked: zone.locked,
                visible: zone.visible,
                stack_id: zone
                    .stack_parent
                    .map(|parent| SmolStr::new(parent.0.to_string())),
                stack_order: 0,
                alias: zone.alias.as_deref().map(ToOwned::to_owned),
                display_mode: zone.display_mode.as_deref().map(SmolStr::new),
                live_folder_path: zone.live_folder_path.as_deref().map(ToOwned::to_owned),
            }
        })
        .collect()
}

pub(super) fn bento_item_from_zone_item(
    zone_id: SmolStr,
    item: &bentodesk_zone::ZoneItem,
    now: &str,
) -> BentoItem {
    BentoItem {
        id: SmolStr::new(item.id.0.to_string()),
        zone_id,
        item_type: rules_item_type_for_path(item.path.as_ref()),
        name: item.name.to_string(),
        path: item.path.to_string(),
        icon_hash: SmolStr::new(item.icon_hash.as_ref()),
        grid_position: GridPosition {
            col: item.x.max(0) as u32,
            row: item.y.max(0) as u32,
            col_span: if item.is_wide { 2 } else { 1 },
        },
        is_wide: item.is_wide,
        added_at: SmolStr::new(now),
        original_path: item.original_path.as_deref().map(ToOwned::to_owned),
        hidden_path: item.hidden_path.as_deref().map(ToOwned::to_owned),
        file_missing: item.file_missing,
        icon_x: None,
        icon_y: None,
        tags: item
            .tags
            .iter()
            .map(|tag| SmolStr::new(tag.as_ref()))
            .collect(),
    }
}

/// Smallest dimension a migrated zone may be shrunk to when clamped onto the
/// viewport. Keeps a zone that started off-screen interactable rather than
/// collapsing it to a 1-px sliver.
pub(super) const MIN_MIGRATED_ZONE_DIMENSION: i32 = 48;

/// Clamp a zone's rect so it lies FULLY within `[0, viewport)` (ROOT-CAUSE
/// -corrupt-zone-geometry.md Part 3). At startup-migration time `app.viewport`
/// is often 0, so the migration falls back to `default_size(Main)` =
/// 1920× 1080; on a smaller logical screen that oversizes zones and pushes
/// high-`x_percent` zones off the right/bottom edge. This guarantees:
/// `x >= 0`, `y >= 0`, `x + w <= vp_w`, `y + h <= vp_h`. Width/height are
/// shrunk only as much as needed (never below `MIN_MIGRATED_ZONE_DIMENSION`),
/// then the origin is pulled back in. Pure / allocation-free for testability.
pub(super) fn clamp_zone_rect_to_viewport(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    viewport: bentodesk_style::Size,
) -> (i32, i32, i32, i32) {
    let vp_w = (viewport.width.max(1.0) as i32).max(MIN_MIGRATED_ZONE_DIMENSION);
    let vp_h = (viewport.height.max(1.0) as i32).max(MIN_MIGRATED_ZONE_DIMENSION);

    // A zone can never be wider/taller than the viewport itself.
    let mut w = w.clamp(MIN_MIGRATED_ZONE_DIMENSION, vp_w);
    let mut h = h.clamp(MIN_MIGRATED_ZONE_DIMENSION, vp_h);

    // Pull the origin in so the (clamped) body fits on-screen, never < 0.
    let x = x.clamp(0, (vp_w - w).max(0));
    let y = y.clamp(0, (vp_h - h).max(0));

    // Final shrink in case the viewport is smaller than the minimum dimension
    // (degenerate screens) so `x + w <= vp_w` / `y + h <= vp_h` always hold.
    if x + w > vp_w {
        w = (vp_w - x).max(1);
    }
    if y + h > vp_h {
        h = (vp_h - y).max(1);
    }
    (x, y, w, h)
}

pub(super) fn zone_list_from_bento_zones(
    zones: &[BentoZone],
    viewport: bentodesk_style::Size,
) -> ZoneList {
    let width = viewport.width.max(1.0);
    let height = viewport.height.max(1.0);
    let mut list = ZoneList::new();
    for (index, source) in zones.iter().enumerate() {
        let id = parse_zone_id(source.id.as_str()).unwrap_or(ZoneId((index + 1) as u64));
        let (zx, zy, zw, zh) = clamp_zone_rect_to_viewport(
            percent_to_dip(source.position.x_percent, width),
            percent_to_dip(source.position.y_percent, height),
            percent_to_dip(source.expanded_size.w_percent, width).max(1),
            percent_to_dip(source.expanded_size.h_percent, height).max(1),
            viewport,
        );
        let mut zone = Zone::new(id, Cow::Owned(source.name.clone()), zx, zy, zw, zh);
        zone.icon = Cow::Owned(source.icon.to_string());
        zone.accent_color = source
            .accent_color
            .as_ref()
            .map(|value| Cow::Owned(value.to_string()));
        zone.grid_columns = source.grid_columns.max(1);
        zone.capsule_size = Cow::Owned(source.capsule_size.to_string());
        zone.capsule_shape = Cow::Owned(source.capsule_shape.to_string());
        zone.locked = source.locked;
        zone.visible = source.visible;
        zone.alias = source.alias.as_ref().map(|value| Cow::Owned(value.clone()));
        zone.display_mode = source
            .display_mode
            .as_ref()
            .map(|value| Cow::Owned(value.to_string()));
        zone.live_folder_path = source
            .live_folder_path
            .as_ref()
            .map(|value| Cow::Owned(value.to_string()));
        zone.stack_parent = source
            .stack_id
            .as_ref()
            .and_then(|value| parse_zone_id(value.as_str()));
        for item in &source.items {
            zone.items.push(zone_item_from_bento_item(item));
        }
        list.add(zone);
    }
    let children: Vec<(ZoneId, ZoneId)> = list
        .iter()
        .filter_map(|zone| zone.stack_parent.map(|parent| (parent, zone.id)))
        .collect();
    for (parent, child) in children {
        if let Some(parent_zone) = list.get_mut(parent) {
            if !parent_zone.stack_members.contains(&child) {
                parent_zone.stack_members.push(child);
            }
        }
    }
    list
}

pub(super) fn zone_item_from_bento_item(item: &BentoItem) -> ZoneItem {
    ZoneItem {
        id: ZoneItemId(stable_id_from_string(item.id.as_str())),
        name: Cow::Owned(item.name.clone()),
        path: Cow::Owned(item.path.clone()),
        icon_hash: Cow::Owned(item.icon_hash.to_string()),
        x: item.grid_position.col as i32,
        y: item.grid_position.row as i32,
        is_wide: item.is_wide,
        file_missing: item.file_missing,
        original_path: item.original_path.clone().map(Cow::Owned),
        hidden_path: item.hidden_path.clone().map(Cow::Owned),
        tags: item
            .tags
            .iter()
            .map(|tag| Cow::Owned(tag.to_string()))
            .collect(),
    }
}

pub(super) fn parse_zone_id(value: &str) -> Option<ZoneId> {
    let raw = stable_id_from_string(value);
    if raw == 0 { None } else { Some(ZoneId(raw)) }
}

/// Wave H5 (2026-05-20) — derive a stable, non-zero u64 from any id string
/// the Tauri 1.2.4 legacy `layout.json` may carry. Tauri persisted item +
/// zone ids as UUID strings (e.g. "af66a432-4e0d-4d7e-a38f-2fc20319269a"),
/// which `str::parse::<u64>` rejects. Returning `ZoneItemId::INVALID = 0`
/// for those entries caused `storage::decode` to silently drop EVERY item
/// migrated from a Tauri snapshot — pills showed `0` counts and the
/// expanded zone body rendered empty even though the file held items.
///
/// Behaviour:
/// - Decimal u64 strings (legacy native persistence) pass through unchanged.
/// - Any other shape (UUID, hex, mixed) is hashed via Rust's
///   `DefaultHasher` to a deterministic u64. The high bit is forced on so
///   the result can never collide with `ZoneItemId::INVALID` (0) or with
///   the small monotonic ids `AppState::alloc_zone_id` hands out at
///   runtime.
pub(super) fn stable_id_from_string(value: &str) -> u64 {
    let trimmed = value.trim();
    if let Ok(parsed) = trimmed.parse::<u64>() {
        if parsed != 0 {
            return parsed;
        }
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    trimmed.hash(&mut hasher);
    hasher.finish() | (1 << 63)
}

pub(super) fn percent_to_dip(percent: f64, total: f32) -> i32 {
    (((percent / 100.0) * f64::from(total)).round() as i32).max(0)
}
