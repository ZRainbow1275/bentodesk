//! Ordered Zone collection, stack relations, and cross-Zone item operations.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StackScatterLayout {
    anchor_x: i32,
    anchor_y: i32,
    anchor_w: i32,
    anchor_h: i32,
    viewport_w: i32,
    viewport_h: i32,
}

fn max_top_left(viewport: i32, extent: i32) -> i32 {
    (viewport.max(1) - extent.max(0)).max(0)
}

fn clamp_top_left(value: i32, viewport: i32, extent: i32) -> i32 {
    value.clamp(0, max_top_left(viewport, extent))
}

/// Collection of zones. Inline-allocated for the steady-state count
/// (BentoDesk users typically maintain ≤ 8 zones; spec §10 says no
/// per-frame heap, and zone iteration runs in render path).
#[derive(Debug, Default, Clone)]
pub struct ZoneList {
    zones: SmallVec<[Zone; 8]>,
}

impl ZoneList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.zones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Append a zone. Caller is responsible for id uniqueness — duplicates
    /// are stored in arrival order (the persistence codec round-trips
    /// faithfully and offers no dedup pass).
    pub fn add(&mut self, zone: Zone) {
        self.zones.push(zone);
    }

    /// Remove the zone with `id`, if present. Returns true on hit. O(n) —
    /// fine for the zone count budget.
    pub fn remove(&mut self, id: ZoneId) -> bool {
        if let Some(idx) = self.zones.iter().position(|z| z.id == id) {
            self.zones.remove(idx);
            true
        } else {
            false
        }
    }

    /// Find a zone by id (read-only).
    pub fn get(&self, id: ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// Find a zone by id for mutation (move / rename / resize).
    pub fn get_mut(&mut self, id: ZoneId) -> Option<&mut Zone> {
        self.zones.iter_mut().find(|z| z.id == id)
    }

    /// Iterate all zones in arrival order.
    pub fn iter(&self) -> core::slice::Iter<'_, Zone> {
        self.zones.iter()
    }

    /// Iterate all zones mutably in arrival order. Mutation is intentionally
    /// explicit so shell command handlers can update item metadata without
    /// exposing the inline storage detail.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, Zone> {
        self.zones.iter_mut()
    }

    /// Reorder the zone identified by `id` to position `idx` in arrival
    /// order. Returns `true` on hit, `false` if `id` is absent. The new
    /// index is clamped to `[0, len-1]` so callers receive saturating —
    /// not panicking — semantics for out-of-range targets.
    ///
    /// Implementation: `remove(old) + insert(idx)` so the relative ordering
    /// of intervening zones is preserved (cheaper than a full sort, and
    /// `swap_remove` would scramble the visible Z-order which is what the
    /// `Command::ReorderZone` UX surface relies on).
    pub fn move_to_index(&mut self, id: ZoneId, idx: usize) -> bool {
        let Some(old_idx) = self.zones.iter().position(|z| z.id == id) else {
            return false;
        };
        let new_idx = idx.min(self.zones.len().saturating_sub(1));
        if new_idx == old_idx {
            return true;
        }
        let zone = self.zones.remove(old_idx);
        self.zones.insert(new_idx, zone);
        true
    }

    /// Fold `child` under `parent` and keep the relation as one flat stack.
    ///
    /// `parent` may name an existing stack member; in that case its visible
    /// anchor remains the target. When `child` is itself a stack anchor, the
    /// whole source stack is transferred and flattened under the target. This
    /// mirrors Tauri's shared `stack_id` model and prevents a zone from being
    /// both a hidden child and a nested anchor.
    pub fn stack(&mut self, parent: ZoneId, child: ZoneId) -> bool {
        if parent == child || self.get(parent).is_none() || self.get(child).is_none() {
            return false;
        }

        let target_anchor = self.stack_anchor_for(parent).unwrap_or(parent);
        let source_anchor = self.stack_anchor_for(child).unwrap_or(child);
        if target_anchor == source_anchor {
            return false;
        }

        let child_is_anchor = self.get(child).is_some_and(Zone::is_stack_anchor);
        let mut transfer = SmallVec::<[ZoneId; 8]>::new();
        transfer.push(child);
        if child_is_anchor {
            let source_members = self
                .get(child)
                .map(|zone| zone.stack_members.clone())
                .unwrap_or_default();
            for member in source_members {
                if self
                    .get(member)
                    .and_then(|zone| zone.stack_parent)
                    .is_some_and(|anchor| anchor == child)
                    && !transfer.contains(&member)
                {
                    transfer.push(member);
                }
            }
        }
        if transfer.contains(&target_anchor) {
            return false;
        }

        // Remove every transferred zone from its previous parent first. The
        // target list is rebuilt below, so no zone can remain in two stacks.
        for member in transfer.iter().copied() {
            let old_parent = self.get(member).and_then(|zone| zone.stack_parent);
            if let Some(old_parent) = old_parent {
                if old_parent == target_anchor {
                    continue;
                }
                if let Some(old_idx) = self.zones.iter().position(|zone| zone.id == old_parent) {
                    self.zones[old_idx].stack_members.retain(|id| *id != member);
                }
            }
        }

        // A transferred source anchor becomes an ordinary child; its former
        // children are transferred beside it rather than remaining nested.
        if child_is_anchor {
            if let Some(source) = self.get_mut(child) {
                source.stack_members.clear();
            }
        }
        for member in transfer.iter().copied() {
            if let Some(zone) = self.get_mut(member) {
                zone.stack_parent = Some(target_anchor);
            }
        }
        let Some(parent_idx) = self.zones.iter().position(|zone| zone.id == target_anchor) else {
            return false;
        };
        for member in transfer {
            if !self.zones[parent_idx].stack_members.contains(&member) {
                self.zones[parent_idx].stack_members.push(member);
            }
        }
        true
    }

    /// Move a free zone or every member of its existing stack by one rigid
    /// delta, preserving all member offsets exactly (Tauri `StackWrapper`).
    pub fn move_group_to(&mut self, id: ZoneId, x: i32, y: i32) -> bool {
        let Some(zone) = self.get(id) else {
            return false;
        };
        let dx = x.saturating_sub(zone.x);
        let dy = y.saturating_sub(zone.y);
        if dx == 0 && dy == 0 {
            return false;
        }

        let anchor = self.stack_anchor_for(id).unwrap_or(id);
        let mut members = self.stack_member_ids(anchor).unwrap_or_else(|| {
            let mut ids = SmallVec::<[ZoneId; 8]>::new();
            ids.push(id);
            ids
        });
        if !members.contains(&id) {
            members.push(id);
        }
        for member in members {
            if let Some(zone) = self.get_mut(member) {
                zone.x = zone.x.saturating_add(dx);
                zone.y = zone.y.saturating_add(dy);
            }
        }
        true
    }

    /// Flatten legacy nested stack relations without disturbing the order of
    /// already-valid stacks.
    ///
    /// Older builds could make a stack anchor a child of another anchor while
    /// leaving its own `stack_members` intact. Move those members immediately
    /// after the former source anchor in its parent stack. Each pass removes
    /// one nested anchor, so the bounded zone-count loop also handles deeper
    /// legacy trees without recursion.
    pub fn flatten_nested_stacks(&mut self) -> bool {
        let mut changed = false;
        for _ in 0..self.zones.len() {
            let Some(source_idx) = self
                .zones
                .iter()
                .position(|zone| zone.stack_parent.is_some() && !zone.stack_members.is_empty())
            else {
                break;
            };
            let source_id = self.zones[source_idx].id;
            let Some(parent_id) = self.zones[source_idx].stack_parent else {
                continue;
            };
            let Some(parent_idx) = self.zones.iter().position(|zone| zone.id == parent_id) else {
                // A missing parent cannot own a visible stack. Promote the
                // source back to an independent anchor instead of hiding it.
                self.zones[source_idx].stack_parent = None;
                changed = true;
                continue;
            };
            if source_id == parent_id {
                self.zones[source_idx].stack_parent = None;
                changed = true;
                continue;
            }
            let mut cursor = parent_id;
            let mut cycle = false;
            for _ in 0..self.zones.len() {
                if cursor == source_id {
                    cycle = true;
                    break;
                }
                let Some(next) = self.get(cursor).and_then(|zone| zone.stack_parent) else {
                    break;
                };
                cursor = next;
            }
            if cycle {
                // Keep the source's member order and promote it to the root;
                // its former parent can then remain a normal child.
                self.zones[source_idx].stack_parent = None;
                changed = true;
                continue;
            }

            let source_members = core::mem::take(&mut self.zones[source_idx].stack_members);
            let mut transfer = SmallVec::<[ZoneId; 8]>::new();
            for member in source_members {
                if member != source_id
                    && member != parent_id
                    && self
                        .get(member)
                        .and_then(|zone| zone.stack_parent)
                        .is_some_and(|owner| owner == source_id)
                    && !transfer.contains(&member)
                {
                    transfer.push(member);
                }
            }

            for member in transfer.iter().copied() {
                if let Some(zone) = self.get_mut(member) {
                    zone.stack_parent = Some(parent_id);
                }
            }
            for zone in &mut self.zones {
                if zone.id != parent_id {
                    zone.stack_members
                        .retain(|member| !transfer.contains(member));
                }
            }

            let parent = &mut self.zones[parent_idx];
            parent
                .stack_members
                .retain(|member| !transfer.contains(member));
            let insert_at = if let Some(source_pos) = parent
                .stack_members
                .iter()
                .position(|member| *member == source_id)
            {
                source_pos + 1
            } else {
                parent.stack_members.push(source_id);
                parent.stack_members.len()
            };
            for (offset, member) in transfer.into_iter().enumerate() {
                parent.stack_members.insert(insert_at + offset, member);
            }
            changed = true;
        }
        changed
    }

    pub fn stack_anchor_for(&self, id: ZoneId) -> Option<ZoneId> {
        let zone = self.get(id)?;
        if zone.is_stack_anchor() {
            return Some(zone.id);
        }
        let parent = zone.stack_parent?;
        self.get(parent)
            .filter(|anchor| anchor.stack_members.contains(&id))
            .map(|anchor| anchor.id)
    }

    pub fn stack_member_ids(&self, anchor: ZoneId) -> Option<SmallVec<[ZoneId; 8]>> {
        let anchor_zone = self.get(anchor)?;
        if !anchor_zone.is_stack_anchor() {
            return None;
        }
        let mut ids = SmallVec::<[ZoneId; 8]>::new();
        ids.push(anchor);
        for member in &anchor_zone.stack_members {
            if self
                .get(*member)
                .and_then(|zone| zone.stack_parent)
                .is_some_and(|parent| parent == anchor)
            {
                ids.push(*member);
            }
        }
        (ids.len() >= 2).then_some(ids)
    }

    pub fn stack_member_ids_for(&self, id: ZoneId) -> Option<SmallVec<[ZoneId; 8]>> {
        let anchor = self.stack_anchor_for(id)?;
        self.stack_member_ids(anchor)
    }

    pub fn detach_from_stack(&mut self, member: ZoneId) -> Option<StackDetachOutcome> {
        let anchor = self.stack_anchor_for(member)?;
        let members = self.stack_member_ids(anchor)?;
        if !members.contains(&member) {
            return None;
        }

        let mut remaining = SmallVec::<[ZoneId; 8]>::new();
        for id in members.iter().copied() {
            if id != member {
                remaining.push(id);
            }
            if let Some(idx) = self.zones.iter().position(|zone| zone.id == id) {
                self.zones[idx].stack_parent = None;
                self.zones[idx].stack_members.clear();
            }
        }

        let new_anchor = if remaining.len() >= 2 {
            let next_anchor = remaining[0];
            for child in remaining.iter().copied().skip(1) {
                let _ = self.stack(next_anchor, child);
            }
            Some(next_anchor)
        } else {
            None
        };

        Some(StackDetachOutcome {
            detached_member: member,
            new_anchor,
            remaining_count: remaining.len(),
        })
    }

    pub fn reorder_stack_member(
        &mut self,
        anchor: ZoneId,
        member: ZoneId,
        target_index: usize,
    ) -> bool {
        if anchor == member {
            return false;
        }
        let Some(anchor_idx) = self.zones.iter().position(|zone| zone.id == anchor) else {
            return false;
        };
        if !self.zones[anchor_idx].is_stack_anchor() {
            return false;
        }
        if self
            .get(member)
            .and_then(|zone| zone.stack_parent)
            .is_none_or(|parent| parent != anchor)
        {
            return false;
        }

        let members = &mut self.zones[anchor_idx].stack_members;
        let Some(from_index) = members.iter().position(|id| *id == member) else {
            return false;
        };
        let before = members.clone();
        let moved = members.remove(from_index);
        let child_target = target_index.saturating_sub(1).min(members.len());
        members.insert(child_target, moved);
        *members != before
    }

    /// Dissolve a stack relationship. Passing a stack anchor releases all
    /// its children; passing a child removes it from its parent only.
    pub fn unstack(&mut self, id: ZoneId) -> bool {
        let Some(idx) = self.zones.iter().position(|z| z.id == id) else {
            return false;
        };

        if !self.zones[idx].stack_members.is_empty() {
            let members = self.zones[idx].stack_members.clone();
            self.zones[idx].stack_members.clear();
            for member in members {
                if let Some(child_idx) = self.zones.iter().position(|z| z.id == member) {
                    self.zones[child_idx].stack_parent = None;
                }
            }
            return true;
        }

        let Some(parent) = self.zones[idx].stack_parent.take() else {
            return false;
        };
        if let Some(parent_idx) = self.zones.iter().position(|z| z.id == parent) {
            self.zones[parent_idx]
                .stack_members
                .retain(|member| *member != id);
        }
        true
    }

    /// Dissolve the stack containing `id` and spread every former member from
    /// the anchor rect so the released zones do not remain visually piled up.
    pub fn dissolve_stack_scattered(
        &mut self,
        id: ZoneId,
        viewport_w: i32,
        viewport_h: i32,
    ) -> bool {
        let Some(anchor) = self.stack_anchor_for(id) else {
            return false;
        };
        let Some(member_ids) = self.stack_member_ids(anchor) else {
            return false;
        };
        let Some(anchor_zone) = self.get(anchor) else {
            return false;
        };
        let anchor_x = anchor_zone.x;
        let anchor_y = anchor_zone.y;
        let anchor_w = anchor_zone.w;
        let anchor_h = anchor_zone.h;

        if !self.unstack(anchor) {
            return false;
        }
        self.scatter_released_stack_members(
            &member_ids,
            StackScatterLayout {
                anchor_x,
                anchor_y,
                anchor_w,
                anchor_h,
                viewport_w,
                viewport_h,
            },
        );
        true
    }

    /// Preserve [`Self::unstack`] semantics while applying scatter only when
    /// `id` is a visible stack anchor. Child-only unstack remains a single
    /// member release.
    pub fn unstack_with_scatter(&mut self, id: ZoneId, viewport_w: i32, viewport_h: i32) -> bool {
        if self.get(id).is_some_and(Zone::is_stack_anchor) {
            return self.dissolve_stack_scattered(id, viewport_w, viewport_h);
        }
        self.unstack(id)
    }

    fn scatter_released_stack_members(
        &mut self,
        member_ids: &[ZoneId],
        layout: StackScatterLayout,
    ) {
        let step_x = layout.anchor_w.max(0) + STACK_SCATTER_GAP_DIP;
        let step_y = layout.anchor_h.max(0) + STACK_SCATTER_GAP_DIP;
        let max_x = max_top_left(layout.viewport_w, layout.anchor_w);

        let mut cursor_x = layout.anchor_x;
        let mut cursor_y = layout.anchor_y;
        for (index, id) in member_ids.iter().copied().enumerate() {
            if index > 0 && cursor_x > max_x {
                cursor_x = layout.anchor_x;
                cursor_y += step_y;
            }
            if let Some(zone) = self.get_mut(id) {
                zone.x = clamp_top_left(cursor_x, layout.viewport_w, zone.w);
                zone.y = clamp_top_left(cursor_y, layout.viewport_h, zone.h);
            }
            cursor_x += step_x;
        }
    }

    pub fn add_item(
        &mut self,
        zone_id: ZoneId,
        path: impl Into<Cow<'static, str>>,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        self.get_mut(zone_id)?.add_item(path, icon_hash)
    }

    pub fn add_item_with_metadata(
        &mut self,
        zone_id: ZoneId,
        path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        icon_hash: impl Into<Cow<'static, str>>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        self.get_mut(zone_id)?.add_item_with_metadata(
            path,
            display_path,
            icon_hash,
            original_path,
            hidden_path,
        )
    }

    pub fn item(&self, zone_id: ZoneId, item_id: ZoneItemId) -> Option<&ZoneItem> {
        self.get(zone_id)?.item(item_id)
    }

    pub fn remove_item(&mut self, zone_id: ZoneId, item_id: ZoneItemId) -> bool {
        self.get_mut(zone_id)
            .is_some_and(|zone| zone.remove_item(item_id))
    }

    pub fn auto_arrange_items(&mut self, zone_id: ZoneId) -> bool {
        self.get_mut(zone_id).is_some_and(Zone::auto_arrange_items)
    }

    pub fn update_item_file_metadata(
        &mut self,
        zone_id: ZoneId,
        item_id: ZoneItemId,
        effective_path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> bool {
        self.get_mut(zone_id).is_some_and(|zone| {
            zone.update_item_file_metadata(
                item_id,
                effective_path,
                display_path,
                original_path,
                hidden_path,
            )
        })
    }

    pub fn move_item(&mut self, zone_id: ZoneId, item_id: ZoneItemId, x: i32, y: i32) -> bool {
        self.get_mut(zone_id)
            .is_some_and(|zone| zone.move_item(item_id, x, y))
    }

    pub fn toggle_item_wide(&mut self, zone_id: ZoneId, item_id: ZoneItemId) -> bool {
        self.get_mut(zone_id)
            .is_some_and(|zone| zone.toggle_item_wide(item_id))
    }

    pub fn move_item_to_zone(
        &mut self,
        from_zone_id: ZoneId,
        to_zone_id: ZoneId,
        item_id: ZoneItemId,
        effective_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> bool {
        if from_zone_id == to_zone_id {
            return self
                .get(from_zone_id)
                .is_some_and(|zone| zone.item(item_id).is_some());
        }
        let Some(from_idx) = self.zones.iter().position(|zone| zone.id == from_zone_id) else {
            return false;
        };
        let Some(to_idx) = self.zones.iter().position(|zone| zone.id == to_zone_id) else {
            return false;
        };
        let Some(item_idx) = self.zones[from_idx]
            .items
            .iter()
            .position(|item| item.id == item_id)
        else {
            return false;
        };
        let mut item = self.zones[from_idx].items.remove(item_idx);
        if let Some(effective_path) = effective_path {
            item.path = effective_path;
        }
        if let Some(hidden_path) = hidden_path {
            item.hidden_path = Some(hidden_path);
        }
        let target_len = self.zones[to_idx].items.len() as i32;
        let columns = self.zones[to_idx].grid_columns.max(1) as i32;
        item.x = target_len % columns;
        item.y = target_len / columns;
        self.zones[to_idx].items.push(item);
        true
    }

    pub fn set_item_icon_hash(
        &mut self,
        zone_id: ZoneId,
        path: &str,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> bool {
        self.get_mut(zone_id)
            .is_some_and(|zone| zone.set_item_icon_hash(path, icon_hash))
    }

    pub fn replace_item_path(
        &mut self,
        old_path: &str,
        new_path: impl Into<Cow<'static, str>>,
    ) -> bool {
        let mut changed = false;
        let new_path = new_path.into();
        for zone in &mut self.zones {
            if zone.replace_item_path(old_path, Cow::Owned(new_path.to_string())) {
                changed = true;
            }
        }
        changed
    }

    pub fn mark_item_missing(&mut self, path: &str, missing: bool) -> bool {
        let mut changed = false;
        for zone in &mut self.zones {
            if zone.mark_item_missing(path, missing) {
                changed = true;
            }
        }
        changed
    }
}
