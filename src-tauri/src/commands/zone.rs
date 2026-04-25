//! Zone CRUD commands.

use tauri::State;

use crate::guardrails;
use crate::hidden_items;
use crate::layout::persistence::{
    BentoZone, LayoutData, RelativePosition, RelativeSize, ZoneUpdate,
};
use crate::timeline::hook as timeline_hook;
use crate::AppState;

#[tauri::command]
pub async fn create_zone(
    state: State<'_, AppState>,
    name: String,
    icon: String,
    position: RelativePosition,
    expanded_size: RelativeSize,
) -> Result<BentoZone, String> {
    // Guardrail: verify zone count stays within the safety envelope.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        guardrails::ensure_can_create_zone(&layout, &settings)?;
    }

    let zone = BentoZone {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        icon,
        position,
        expanded_size,
        items: Vec::new(),
        accent_color: None,
        sort_order: 0,
        auto_group: None,
        grid_columns: 4,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        capsule_size: "medium".to_string(),
        capsule_shape: "pill".to_string(),
        locked: false,
        stack_id: None,
        stack_order: 0,
        alias: None,
        display_mode: None,
        live_folder_path: None,
    };

    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let mut zone = zone;
        zone.sort_order = layout.zones.len() as i32;
        layout.zones.push(zone.clone());
        layout.last_modified = chrono::Utc::now().to_rfc3339();
        zone
    };
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_create");

    Ok(result)
}

#[tauri::command]
pub async fn update_zone(
    state: State<'_, AppState>,
    id: String,
    updates: ZoneUpdate,
) -> Result<BentoZone, String> {
    let result = {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == id)
            .ok_or_else(|| format!("Zone not found: {id}"))?;

        if let Some(name) = updates.name {
            zone.name = name;
        }
        if let Some(icon) = updates.icon {
            zone.icon = icon;
        }
        if let Some(position) = updates.position {
            zone.position = position;
        }
        if let Some(expanded_size) = updates.expanded_size {
            zone.expanded_size = expanded_size;
        }
        if let Some(accent_color) = updates.accent_color {
            zone.accent_color = Some(accent_color);
        }
        if let Some(grid_columns) = updates.grid_columns {
            zone.grid_columns = grid_columns;
        }
        if let Some(auto_group) = updates.auto_group {
            zone.auto_group = Some(auto_group);
        }
        if let Some(capsule_size) = updates.capsule_size {
            zone.capsule_size = capsule_size;
        }
        if let Some(capsule_shape) = updates.capsule_shape {
            zone.capsule_shape = capsule_shape;
        }
        if let Some(locked) = updates.locked {
            zone.locked = locked;
        }
        if let Some(alias_opt) = updates.alias {
            // Inner None clears the alias; inner Some("…") sets it.
            zone.alias = alias_opt.and_then(|s| if s.is_empty() { None } else { Some(s) });
        }
        if let Some(mode_opt) = updates.display_mode {
            zone.display_mode = mode_opt.and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
        }
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        let cloned = zone.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();

        cloned
    };
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_update");

    Ok(result)
}

#[tauri::command]
pub async fn delete_zone(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // SAFETY: Restore all zone items (move from .bentodesk/ back to Desktop)
    // BEFORE removing from layout. This ensures files become visible again.
    {
        let layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter()
            .find(|z| z.id == id)
            .ok_or_else(|| format!("Zone not found: {id}"))?;

        let items = zone.items.clone();
        drop(layout); // Release lock before doing file I/O

        let report = hidden_items::restore_zone_items(&state.app_handle, &items);
        tracing::info!(
            "delete_zone '{}': restored {}/{} hidden items before deletion ({} skipped)",
            id,
            report.restored,
            items.len(),
            report.skipped.len()
        );
    }

    // Clean up the now-empty zone subdirectory
    hidden_items::cleanup_zone_dir(&state.app_handle, &id);

    // Now remove the zone from layout
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let len_before = layout.zones.len();
        layout.zones.retain(|z| z.id != id);
        if layout.zones.len() == len_before {
            return Err(format!("Zone not found: {id}"));
        }
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_delete");
    Ok(())
}

#[tauri::command]
pub async fn list_zones(state: State<'_, AppState>) -> Result<Vec<BentoZone>, String> {
    let layout = state.layout.lock().map_err(|e| e.to_string())?;
    Ok(layout.zones.clone())
}

#[tauri::command]
pub async fn reorder_zones(
    state: State<'_, AppState>,
    zone_ids: Vec<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        for (i, id) in zone_ids.iter().enumerate() {
            if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
                zone.sort_order = i as i32;
            }
        }
        layout.zones.sort_by_key(|z| z.sort_order);
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_reorder");
    Ok(())
}

// ─── D2 Stack Mode commands ──────────────────────────────────

/// Group two or more zones into a single stack. Returns the allocated stack_id.
///
/// Semantics:
/// - Rejects `zone_ids` with fewer than 2 entries — a stack of one is just a
///   free-standing zone with a stale badge, which `normalize_zone_layout`
///   would have to clean up later. Reject up front.
/// - If any of the zones already belong to different stacks, they are merged
///   into a single new stack with a fresh id (safer than silently preserving
///   a partial stack).
/// - `stack_order` is assigned in the order received (first id → order 0).
#[tauri::command]
pub async fn stack_zones(
    state: State<'_, AppState>,
    zone_ids: Vec<String>,
) -> Result<String, String> {
    let stack_id = uuid::Uuid::new_v4().to_string();
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        apply_stack_zones(&mut layout, &zone_ids, &stack_id)?;
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_stack");
    Ok(stack_id)
}

/// Mutate `layout` to assign every zone in `zone_ids` to `stack_id` with
/// contiguous `stack_order` values starting at 0. Rejects fewer than 2
/// matched zones so a singleton "stack" never lands on disk.
///
/// Pure (no IO, no Tauri state) so the lock-mutate-persist contract can be
/// unit-tested without spinning up an `AppState`.
pub(crate) fn apply_stack_zones(
    layout: &mut LayoutData,
    zone_ids: &[String],
    stack_id: &str,
) -> Result<(), String> {
    if zone_ids.len() < 2 {
        return Err(format!(
            "stack_zones requires at least 2 zone_ids, got {}",
            zone_ids.len()
        ));
    }
    let matched_count = zone_ids
        .iter()
        .filter(|id| layout.zones.iter().any(|z| &&z.id == id))
        .count();
    if matched_count < 2 {
        return Err(format!(
            "stack_zones matched only {matched_count} zone(s), need at least 2"
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut order: u32 = 0;
    for id in zone_ids {
        if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
            zone.stack_id = Some(stack_id.to_string());
            zone.stack_order = order;
            zone.updated_at = now.clone();
            order += 1;
        }
    }
    layout.last_modified = now;
    Ok(())
}

/// Dissolve a stack: clears `stack_id` and resets `stack_order` on every
/// member so they become free-standing zones again. No-op if the stack
/// has no members (e.g. it was already dissolved).
#[tauri::command]
pub async fn unstack_zones(state: State<'_, AppState>, stack_id: String) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        apply_unstack_zones(&mut layout, &stack_id);
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_unstack");
    Ok(())
}

/// Mutate `layout` in place to clear the stack badge from every member of
/// `stack_id`. No-op when the stack has already been dissolved.
pub(crate) fn apply_unstack_zones(layout: &mut LayoutData, stack_id: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut touched = false;
    for zone in layout.zones.iter_mut() {
        if zone.stack_id.as_deref() == Some(stack_id) {
            zone.stack_id = None;
            zone.stack_order = 0;
            zone.updated_at = now.clone();
            touched = true;
        }
    }
    if touched {
        layout.last_modified = now;
    }
}

/// Set or clear the display alias for a zone. Passing `None` clears the
/// alias; passing an empty string is treated as a clear so the UI can
/// round-trip a cleared textbox without a separate "delete" command.
#[tauri::command]
pub async fn set_zone_alias(
    state: State<'_, AppState>,
    zone_id: String,
    alias: Option<String>,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        let zone = layout
            .zones
            .iter_mut()
            .find(|z| z.id == zone_id)
            .ok_or_else(|| format!("Zone not found: {zone_id}"))?;
        zone.alias = alias.and_then(|s| if s.trim().is_empty() { None } else { Some(s) });
        zone.updated_at = chrono::Utc::now().to_rfc3339();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "zone_alias");
    Ok(())
}

/// Move a zone to a new `stack_order` within its stack. Remaining members
/// are re-sequenced to fill the gap so the stack stays contiguous.
#[tauri::command]
pub async fn reorder_stack(
    state: State<'_, AppState>,
    stack_id: String,
    zone_id: String,
    new_order: u32,
) -> Result<(), String> {
    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        apply_reorder_stack(&mut layout, &stack_id, &zone_id, new_order)?;
    }
    state.persist_layout();
    timeline_hook::record_change(&state.app_handle, "stack_reorder");
    Ok(())
}

/// Pure mutation: move `zone_id` to `new_order` within `stack_id` and
/// re-sequence the remaining members so the stack stays contiguous.
pub(crate) fn apply_reorder_stack(
    layout: &mut LayoutData,
    stack_id: &str,
    zone_id: &str,
    new_order: u32,
) -> Result<(), String> {
    let mut members: Vec<(String, u32)> = layout
        .zones
        .iter()
        .filter(|z| z.stack_id.as_deref() == Some(stack_id))
        .map(|z| (z.id.clone(), z.stack_order))
        .collect();
    if members.is_empty() {
        return Err(format!("Stack not found: {stack_id}"));
    }
    if !members.iter().any(|(id, _)| id == zone_id) {
        return Err(format!("Zone {zone_id} is not in stack {stack_id}"));
    }
    members.sort_by_key(|(_, order)| *order);
    members.retain(|(id, _)| id != zone_id);
    let target = (new_order as usize).min(members.len());
    members.insert(target, (zone_id.to_string(), 0));

    let now = chrono::Utc::now().to_rfc3339();
    for (i, (id, _)) in members.iter().enumerate() {
        if let Some(zone) = layout.zones.iter_mut().find(|z| &z.id == id) {
            zone.stack_order = i as u32;
            zone.updated_at = now.clone();
        }
    }
    layout.last_modified = now;
    Ok(())
}

#[cfg(test)]
mod stack_tests {
    // NOTE: tests for the in-memory state transitions live here; full IPC
    // integration tests live in Theme D's test matrix.
    use super::{apply_reorder_stack, apply_stack_zones, apply_unstack_zones};
    use crate::layout::persistence::{BentoZone, LayoutData, RelativePosition, RelativeSize};

    fn sample_zone(id: &str) -> BentoZone {
        BentoZone {
            id: id.to_string(),
            name: "zone".to_string(),
            icon: "folder".to_string(),
            position: RelativePosition {
                x_percent: 0.0,
                y_percent: 0.0,
            },
            expanded_size: RelativeSize {
                w_percent: 25.0,
                h_percent: 40.0,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
            locked: false,
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

    #[test]
    fn stack_fields_serialize_additively() {
        let mut zone = sample_zone("z1");
        // When None / 0 / None, serialization must omit the fields so older
        // clients ignore them cleanly.
        let json = serde_json::to_string(&zone).unwrap();
        assert!(!json.contains("stack_id"));
        assert!(!json.contains("stack_order"));
        assert!(!json.contains("alias"));
        // When populated, fields must appear.
        zone.stack_id = Some("s1".to_string());
        zone.stack_order = 2;
        zone.alias = Some("MyAlias".to_string());
        let json = serde_json::to_string(&zone).unwrap();
        assert!(json.contains("\"stack_id\":\"s1\""));
        assert!(json.contains("\"stack_order\":2"));
        assert!(json.contains("\"alias\":\"MyAlias\""));
    }

    fn layout_with(zones: Vec<BentoZone>) -> LayoutData {
        LayoutData {
            zones,
            ..LayoutData::default()
        }
    }

    /// Spec contract — `stack_zones` with 3 free-standing zones must mark
    /// every member with the same fresh `stack_id` and assign contiguous
    /// `stack_order` 0/1/2 in receive order.
    #[test]
    fn stack_zones_three_zones_assigns_uniform_id_and_contiguous_order() {
        let mut layout = layout_with(vec![
            sample_zone("a"),
            sample_zone("b"),
            sample_zone("c"),
        ]);
        let stack_id = "stack-fresh";

        apply_stack_zones(
            &mut layout,
            &["a".to_string(), "b".to_string(), "c".to_string()],
            stack_id,
        )
        .expect("3 free zones must stack cleanly");

        for zone in &layout.zones {
            assert_eq!(
                zone.stack_id.as_deref(),
                Some(stack_id),
                "every member must carry the new stack_id"
            );
        }
        assert_eq!(layout.zones[0].stack_order, 0);
        assert_eq!(layout.zones[1].stack_order, 1);
        assert_eq!(layout.zones[2].stack_order, 2);
    }

    /// Spec contract — `unstack_zones` clears the badge from every member
    /// (`stack_id = None`, `stack_order = 0`) and leaves zones outside the
    /// dissolved stack untouched.
    #[test]
    fn unstack_zones_clears_badges_on_every_member() {
        let mut a = sample_zone("a");
        a.stack_id = Some("doomed".to_string());
        a.stack_order = 0;
        let mut b = sample_zone("b");
        b.stack_id = Some("doomed".to_string());
        b.stack_order = 1;
        let mut c = sample_zone("c");
        c.stack_id = Some("doomed".to_string());
        c.stack_order = 2;
        // A bystander on a different stack must NOT be touched.
        let mut bystander = sample_zone("by");
        bystander.stack_id = Some("other".to_string());
        bystander.stack_order = 5;

        let mut layout = layout_with(vec![a, b, c, bystander]);

        apply_unstack_zones(&mut layout, "doomed");

        for id in ["a", "b", "c"] {
            let zone = layout.zones.iter().find(|z| z.id == id).unwrap();
            assert!(
                zone.stack_id.is_none(),
                "{id} stack_id must clear after dissolve"
            );
            assert_eq!(zone.stack_order, 0, "{id} stack_order must reset to 0");
        }

        let bystander = layout.zones.iter().find(|z| z.id == "by").unwrap();
        assert_eq!(bystander.stack_id.as_deref(), Some("other"));
        assert_eq!(bystander.stack_order, 5);
    }

    /// Spec contract — `reorder_stack` swap of two members must produce
    /// contiguous `stack_order` values that reflect the new sequence.
    #[test]
    fn reorder_stack_swaps_two_members_and_keeps_sequence_contiguous() {
        let mut a = sample_zone("a");
        a.stack_id = Some("group".to_string());
        a.stack_order = 0;
        let mut b = sample_zone("b");
        b.stack_id = Some("group".to_string());
        b.stack_order = 1;
        let mut c = sample_zone("c");
        c.stack_id = Some("group".to_string());
        c.stack_order = 2;

        let mut layout = layout_with(vec![a, b, c]);

        // Move "a" from order 0 to order 2 -> sequence becomes b(0), c(1), a(2).
        apply_reorder_stack(&mut layout, "group", "a", 2)
            .expect("a must be reorderable inside its own stack");

        let by_id = |id: &str| layout.zones.iter().find(|z| z.id == id).unwrap();
        assert_eq!(by_id("b").stack_order, 0);
        assert_eq!(by_id("c").stack_order, 1);
        assert_eq!(by_id("a").stack_order, 2);

        // Continuity: the set of stack_order values is exactly {0,1,2}, no gaps.
        let mut orders: Vec<u32> = layout.zones.iter().map(|z| z.stack_order).collect();
        orders.sort_unstable();
        assert_eq!(orders, vec![0, 1, 2]);
    }

    /// Spec contract — `stack_zones` with fewer than 2 ids must be rejected
    /// because a stack of one is degenerate. Two reject paths: (a) the
    /// caller hands in a single id, (b) only one of the ids actually
    /// resolves against the layout.
    #[test]
    fn stack_zones_rejects_singleton_input() {
        let mut layout = layout_with(vec![sample_zone("solo")]);

        // Path (a): caller passes a single id.
        let err = apply_stack_zones(&mut layout, &["solo".to_string()], "stk-1")
            .expect_err("a single zone must not form a stack");
        assert!(
            err.contains("at least 2"),
            "rejection must explain the < 2 contract, got: {err}"
        );
        assert!(
            layout.zones[0].stack_id.is_none(),
            "rejected stack must not leak a partial badge"
        );

        // Path (b): caller passes 2 ids but only one resolves.
        let err = apply_stack_zones(
            &mut layout,
            &["solo".to_string(), "ghost".to_string()],
            "stk-2",
        )
        .expect_err("ghost id cannot count toward the 2-zone minimum");
        assert!(err.contains("matched only 1"), "got: {err}");
        assert!(layout.zones[0].stack_id.is_none());
    }

    /// Spec contract — when one of the zones already belongs to a different
    /// stack, `stack_zones` must rebrand it to the new stack rather than
    /// silently keeping the old badge. The new stack is the source of truth.
    #[test]
    fn stack_zones_transfers_zone_from_an_existing_stack() {
        let mut a = sample_zone("a");
        a.stack_id = Some("old-stack".to_string());
        a.stack_order = 7;
        let b = sample_zone("b");
        // Bystander still on old-stack so unstack on "old" would not be a no-op.
        let mut keeper = sample_zone("keeper");
        keeper.stack_id = Some("old-stack".to_string());
        keeper.stack_order = 0;

        let mut layout = layout_with(vec![a, b, keeper]);

        let new_stack = "new-stack";
        apply_stack_zones(
            &mut layout,
            &["a".to_string(), "b".to_string()],
            new_stack,
        )
        .expect("re-stacking must succeed");

        let by_id = |id: &str| layout.zones.iter().find(|z| z.id == id).unwrap();
        assert_eq!(
            by_id("a").stack_id.as_deref(),
            Some(new_stack),
            "transferred zone must carry the new stack_id"
        );
        assert_eq!(by_id("a").stack_order, 0);
        assert_eq!(by_id("b").stack_id.as_deref(), Some(new_stack));
        assert_eq!(by_id("b").stack_order, 1);

        // The remaining bystander on the OLD stack is untouched.
        let keeper = by_id("keeper");
        assert_eq!(keeper.stack_id.as_deref(), Some("old-stack"));
        assert_eq!(keeper.stack_order, 0);
    }

    #[test]
    fn stack_fields_default_on_legacy_payload() {
        // A zone payload missing stack_id/stack_order/alias (pre-1.2) must
        // deserialize with the default values.
        let legacy = r#"{
            "id":"z1","name":"n","icon":"folder",
            "position":{"x_percent":0.0,"y_percent":0.0},
            "expanded_size":{"w_percent":25.0,"h_percent":40.0},
            "items":[],"accent_color":null,"sort_order":0,"auto_group":null,
            "grid_columns":4,
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z",
            "capsule_size":"medium","capsule_shape":"pill"
        }"#;
        let zone: BentoZone = serde_json::from_str(legacy).unwrap();
        assert!(zone.stack_id.is_none());
        assert_eq!(zone.stack_order, 0);
        assert!(zone.alias.is_none());
    }
}
