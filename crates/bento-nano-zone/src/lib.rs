#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-zone` — zone domain model.
//!
//! A *zone* is BentoDesk's user-defined desktop region — a rectangular area
//! holding shortcuts / widgets / drop targets. This crate defines the pure
//! domain types; persistence lives in `bento-nano-platform::storage` (Ruling
//! 1) and rendering in `bento-nano-app::render` (Ruling 2).
//!
//! Dependency rule: this crate may depend on `bento-nano-style` (Color,
//! Rect, Length) and nothing else inside the workspace. No tree, widget,
//! platform, or app deps — keeps the domain reusable from the storage
//! codec without circular reach-back.

#![forbid(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;
use std::path::Path;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Stable identifier for a zone. `u64` because the on-disk schema reserves
/// that width (Ruling 1 wire format) and a 64-bit monotonic counter survives
/// any realistic creation rate over the product's lifetime.
///
/// `serde` derives carry the ΔB ruling (master-decomposition §11) — every
/// type that flows through the dispatcher Command surface preserves a JSON
/// shape for v2.x scripting forward-compat, even though Phase 1 never
/// serializes at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ZoneId(pub u64);

impl ZoneId {
    /// Sentinel reserved for "not yet assigned" — never written to disk.
    pub const INVALID: ZoneId = ZoneId(0);
}

/// Stable identifier for an item inside a single zone. The id is scoped to
/// its owning [`Zone`] so moving an item does not require a global allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ZoneItemId(pub u64);

impl ZoneItemId {
    pub const INVALID: ZoneItemId = ZoneItemId(0);
}

pub const DEFAULT_ZONE_ICON: &str = "folder";
pub const DEFAULT_ZONE_GRID_COLUMNS: u32 = 4;
pub const DEFAULT_ZONE_CAPSULE_SIZE: &str = "medium";
pub const DEFAULT_ZONE_CAPSULE_SHAPE: &str = "pill";
pub const DEFAULT_ZONE_DISPLAY_MODE: &str = "hover";
pub const STACK_SCATTER_GAP_DIP: i32 = 16;

/// One desktop item captured inside a zone.
///
/// This is the selected-stack replacement for the Tauri `BentoItem` shape's
/// runtime-critical fields: id, display name, effective filesystem path,
/// icon cache key, grid position, width hint, and missing-file state. The
/// hide/restore path from 1.x remains a later native file-operation wave;
/// this model is intentionally enough to make Add/Remove/Move reachable and
/// persistent without faking file data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneItem {
    pub id: ZoneItemId,
    pub name: Cow<'static, str>,
    pub path: Cow<'static, str>,
    pub icon_hash: Cow<'static, str>,
    pub x: i32,
    pub y: i32,
    pub is_wide: bool,
    pub file_missing: bool,
    pub original_path: Option<Cow<'static, str>>,
    pub hidden_path: Option<Cow<'static, str>>,
    pub tags: SmallVec<[Cow<'static, str>; 4]>,
}

impl ZoneItem {
    pub fn new(
        id: ZoneItemId,
        path: impl Into<Cow<'static, str>>,
        icon_hash: impl Into<Cow<'static, str>>,
        x: i32,
        y: i32,
    ) -> Self {
        let path = path.into();
        let name = Cow::Owned(display_name_for_path(path.as_ref()));
        Self {
            id,
            name,
            path,
            icon_hash: icon_hash.into(),
            x,
            y,
            is_wide: false,
            file_missing: false,
            original_path: None,
            hidden_path: None,
            tags: SmallVec::new(),
        }
    }
}

/// 1.x-compatible display-name rule: `.lnk` and `.url` shortcuts render
/// without the extension; all other files keep their file name as-is.
pub fn display_name_for_path(path: &str) -> String {
    let file_name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned());
    let len = file_name.len();
    if len >= 4 {
        let suffix = &file_name.as_bytes()[len - 4..];
        if suffix.eq_ignore_ascii_case(b".lnk") || suffix.eq_ignore_ascii_case(b".url") {
            return file_name[..len - 4].to_owned();
        }
    }
    file_name
}

/// One zone. Coordinates are in screen-space DIPs (i32 to match the
/// persisted schema; Win32 `SetWindowPos` consumes i32 anyway).
///
/// `title` is `Cow<'static, str>` so a bundled default zone can borrow from
/// a string literal (zero allocation), while user-named zones own their
/// `String`. The `'static` bound matches `bento-nano-widget::TextNode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    pub id: ZoneId,
    pub title: Cow<'static, str>,
    pub icon: Cow<'static, str>,
    pub accent_color: Option<Cow<'static, str>>,
    /// `false` means the zone remains persisted and manageable from bulk
    /// tools, but is skipped by canvas rendering and hit-testing.
    #[serde(default = "default_zone_visible")]
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub grid_columns: u32,
    pub capsule_size: Cow<'static, str>,
    pub capsule_shape: Cow<'static, str>,
    /// When true, user layout gestures and selected-stack bulk layout/move
    /// helpers leave this zone in place until it is explicitly unlocked.
    #[serde(default)]
    pub locked: bool,
    /// User-facing bulk alias. When set, list/panel surfaces prefer this over
    /// the canonical title without destroying the title itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<Cow<'static, str>>,
    /// Per-zone display-mode override (`hover`, `always`, `click`). `None`
    /// inherits the process default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_mode: Option<Cow<'static, str>>,
    /// Folder mirrored into this zone as a read-only live view. `None` means
    /// the zone owns its normal persisted item list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_folder_path: Option<Cow<'static, str>>,
    /// Parent stack anchor when this zone is visually folded into another
    /// zone. `None` means the zone is rendered independently.
    pub stack_parent: Option<ZoneId>,
    /// Child zones folded under this zone. Inline cap keeps the common
    /// 2-4 member stack allocation-free.
    pub stack_members: SmallVec<[ZoneId; 4]>,
    /// Desktop items captured into this zone. Inline cap covers the common
    /// case and keeps render iteration allocation-free.
    pub items: SmallVec<[ZoneItem; 16]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackDetachOutcome {
    pub detached_member: ZoneId,
    pub new_anchor: Option<ZoneId>,
    pub remaining_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StackScatterLayout {
    anchor_x: i32,
    anchor_y: i32,
    anchor_w: i32,
    anchor_h: i32,
    viewport_w: i32,
    viewport_h: i32,
}

impl Zone {
    /// Construct a zone with a borrowed title.
    pub fn new(
        id: ZoneId,
        title: impl Into<Cow<'static, str>>,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            icon: Cow::Borrowed(DEFAULT_ZONE_ICON),
            accent_color: None,
            visible: true,
            x,
            y,
            w,
            h,
            grid_columns: DEFAULT_ZONE_GRID_COLUMNS,
            capsule_size: Cow::Borrowed(DEFAULT_ZONE_CAPSULE_SIZE),
            capsule_shape: Cow::Borrowed(DEFAULT_ZONE_CAPSULE_SHAPE),
            locked: false,
            alias: None,
            display_mode: None,
            live_folder_path: None,
            stack_parent: None,
            stack_members: SmallVec::new(),
            items: SmallVec::new(),
        }
    }

    pub fn is_stack_anchor(&self) -> bool {
        !self.stack_members.is_empty()
    }

    pub fn is_stacked_child(&self) -> bool {
        self.stack_parent.is_some()
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) -> bool {
        if self.visible == visible {
            return false;
        }
        self.visible = visible;
        true
    }

    fn next_item_id(&self) -> Option<ZoneItemId> {
        self.items
            .iter()
            .map(|item| item.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(ZoneItemId)
    }

    pub fn add_item(
        &mut self,
        path: impl Into<Cow<'static, str>>,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        self.add_item_with_metadata(path, None, icon_hash, None, None)
    }

    pub fn add_item_with_metadata(
        &mut self,
        path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        icon_hash: impl Into<Cow<'static, str>>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> Option<ZoneItemId> {
        let id = self.next_item_id()?;
        let columns = self.grid_columns.max(1) as i32;
        let col = self.items.len() as i32 % columns;
        let row = self.items.len() as i32 / columns;
        let mut item = ZoneItem::new(id, path, icon_hash, col, row);
        if let Some(display_path) = display_path {
            item.name = Cow::Owned(display_name_for_path(display_path));
        }
        item.original_path = original_path;
        item.hidden_path = hidden_path;
        self.items.push(item);
        Some(id)
    }

    pub fn item(&self, id: ZoneItemId) -> Option<&ZoneItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn remove_item(&mut self, id: ZoneItemId) -> bool {
        let Some(idx) = self.items.iter().position(|item| item.id == id) else {
            return false;
        };
        self.items.remove(idx);
        true
    }

    pub fn update_item_file_metadata(
        &mut self,
        id: ZoneItemId,
        effective_path: impl Into<Cow<'static, str>>,
        display_path: Option<&str>,
        original_path: Option<Cow<'static, str>>,
        hidden_path: Option<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let effective_path = effective_path.into();
        item.name = Cow::Owned(display_name_for_path(
            display_path.unwrap_or(effective_path.as_ref()),
        ));
        item.path = effective_path;
        item.original_path = original_path;
        item.hidden_path = hidden_path;
        item.file_missing = false;
        true
    }

    pub fn move_item(&mut self, id: ZoneItemId, x: i32, y: i32) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.x = x.max(0);
        item.y = y.max(0);
        true
    }

    pub fn toggle_item_wide(&mut self, id: ZoneItemId) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.is_wide = !item.is_wide;
        true
    }

    pub fn set_item_icon_hash(
        &mut self,
        path: &str,
        icon_hash: impl Into<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == path)
        else {
            return false;
        };
        item.icon_hash = icon_hash.into();
        true
    }

    pub fn set_icon(&mut self, icon: impl Into<Cow<'static, str>>) {
        self.icon = icon.into();
    }

    pub fn set_accent_color(&mut self, accent_color: Option<Cow<'static, str>>) {
        self.accent_color = accent_color;
    }

    pub fn set_grid_columns(&mut self, grid_columns: u32) {
        self.grid_columns = grid_columns.max(1);
    }

    pub fn set_capsule_size(&mut self, capsule_size: impl Into<Cow<'static, str>>) {
        self.capsule_size = capsule_size.into();
    }

    pub fn set_capsule_shape(&mut self, capsule_shape: impl Into<Cow<'static, str>>) {
        self.capsule_shape = capsule_shape.into();
    }

    pub fn set_capsule(
        &mut self,
        capsule_size: impl Into<Cow<'static, str>>,
        capsule_shape: impl Into<Cow<'static, str>>,
    ) {
        self.capsule_size = capsule_size.into();
        self.capsule_shape = capsule_shape.into();
    }

    pub fn set_locked(&mut self, locked: bool) -> bool {
        if self.locked == locked {
            return false;
        }
        self.locked = locked;
        true
    }

    pub fn set_alias(&mut self, alias: Option<Cow<'static, str>>) -> bool {
        if self.alias == alias {
            return false;
        }
        self.alias = alias;
        true
    }

    pub fn set_display_mode(&mut self, display_mode: Option<Cow<'static, str>>) -> bool {
        if self.display_mode == display_mode {
            return false;
        }
        self.display_mode = display_mode;
        true
    }

    pub fn set_live_folder_path(&mut self, live_folder_path: Option<Cow<'static, str>>) -> bool {
        if self.live_folder_path == live_folder_path {
            return false;
        }
        self.live_folder_path = live_folder_path;
        true
    }

    pub fn replace_item_path(
        &mut self,
        old_path: &str,
        new_path: impl Into<Cow<'static, str>>,
    ) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == old_path)
        else {
            return false;
        };
        let new_path = new_path.into();
        item.name = Cow::Owned(display_name_for_path(new_path.as_ref()));
        item.path = new_path;
        item.file_missing = false;
        true
    }

    pub fn mark_item_missing(&mut self, path: &str, missing: bool) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.path.as_ref() == path)
        else {
            return false;
        };
        item.file_missing = missing;
        true
    }
}

fn default_zone_visible() -> bool {
    true
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

    /// Fold `child` under `parent` and make `parent` the visible stack
    /// anchor. Returns `false` when either id is missing or both ids match.
    pub fn stack(&mut self, parent: ZoneId, child: ZoneId) -> bool {
        if parent == child {
            return false;
        }
        let Some(parent_idx) = self.zones.iter().position(|z| z.id == parent) else {
            return false;
        };
        let Some(child_idx) = self.zones.iter().position(|z| z.id == child) else {
            return false;
        };

        if let Some(old_parent) = self.zones[child_idx].stack_parent {
            if let Some(old_idx) = self.zones.iter().position(|z| z.id == old_parent) {
                self.zones[old_idx].stack_members.retain(|id| *id != child);
            }
        }

        self.zones[child_idx].stack_parent = Some(parent);
        if !self.zones[parent_idx].stack_members.contains(&child) {
            self.zones[parent_idx].stack_members.push(child);
        }
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(id: u64, x: i32) -> Zone {
        Zone::new(ZoneId(id), Cow::Borrowed("z"), x, 0, 100, 100)
    }

    #[test]
    fn zone_list_add_and_iter_preserve_order() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));
        let xs: Vec<i32> = zl.iter().map(|z| z.x).collect();
        assert_eq!(xs, vec![10, 20, 30]);
        assert_eq!(zl.len(), 3);
    }

    #[test]
    fn zone_list_remove_returns_true_on_hit() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        assert!(zl.remove(ZoneId(1)));
        assert_eq!(zl.len(), 1);
        assert!(zl.get(ZoneId(1)).is_none());
        assert!(zl.get(ZoneId(2)).is_some());
        assert!(!zl.remove(ZoneId(99)), "missing id must report false");
    }

    #[test]
    fn zone_list_get_mut_allows_geometry_edit() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        let z = match zl.get_mut(ZoneId(1)) {
            Some(z) => z,
            None => return,
        };
        z.x = 999;
        z.w = 200;
        let z2 = match zl.get(ZoneId(1)) {
            Some(z) => z,
            None => return,
        };
        assert_eq!(z2.x, 999);
        assert_eq!(z2.w, 200);
    }

    #[test]
    fn zone_id_invalid_is_sentinel_zero() {
        assert_eq!(ZoneId::INVALID, ZoneId(0));
        let zl = ZoneList::new();
        assert!(zl.is_empty());
    }

    #[test]
    fn zone_visibility_defaults_visible_and_reports_changes() {
        let mut z = zone(7, 0);
        assert!(z.is_visible());
        assert!(z.set_visible(false));
        assert!(!z.is_visible());
        assert!(!z.set_visible(false));
        assert!(z.set_visible(true));
        assert!(z.is_visible());
    }

    #[test]
    fn zone_bulk_metadata_defaults_and_reports_changes() {
        let mut z = zone(8, 0);
        assert!(!z.locked);
        assert!(z.alias.is_none());
        assert!(z.display_mode.is_none());

        assert!(z.set_locked(true));
        assert!(!z.set_locked(true));
        assert!(z.locked);
        assert!(z.set_locked(false));
        assert!(!z.locked);

        assert!(z.set_alias(Some(Cow::Borrowed("Alias"))));
        assert!(!z.set_alias(Some(Cow::Borrowed("Alias"))));
        assert_eq!(z.alias.as_deref(), Some("Alias"));
        assert!(z.set_alias(None));
        assert!(z.alias.is_none());

        assert!(z.set_display_mode(Some(Cow::Borrowed("hover"))));
        assert!(!z.set_display_mode(Some(Cow::Borrowed("hover"))));
        assert_eq!(z.display_mode.as_deref(), Some("hover"));
        assert!(z.set_display_mode(None));
        assert!(z.display_mode.is_none());
    }

    fn ordered_ids(zl: &ZoneList) -> Vec<u64> {
        zl.iter().map(|z| z.id.0).collect()
    }

    #[test]
    fn move_to_index_to_smaller_index_shifts_intervening_zones() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));
        zl.add(zone(4, 40));

        assert!(zl.move_to_index(ZoneId(4), 1));
        assert_eq!(
            ordered_ids(&zl),
            vec![1, 4, 2, 3],
            "moving id=4 to idx=1 must insert before former idx-1 zone"
        );
    }

    #[test]
    fn move_to_index_to_larger_index_shifts_intervening_zones() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));
        zl.add(zone(4, 40));

        assert!(zl.move_to_index(ZoneId(1), 2));
        assert_eq!(
            ordered_ids(&zl),
            vec![2, 3, 1, 4],
            "moving id=1 to idx=2 must land it after the original idx-2 zone slot"
        );
    }

    #[test]
    fn move_to_index_missing_id_returns_false_and_preserves_order() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));

        assert!(!zl.move_to_index(ZoneId(999), 0));
        assert_eq!(ordered_ids(&zl), vec![1, 2]);
    }

    #[test]
    fn move_to_index_clamps_oversized_idx_to_last_slot() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.move_to_index(ZoneId(1), 99));
        assert_eq!(
            ordered_ids(&zl),
            vec![2, 3, 1],
            "idx > len-1 must clamp the moved zone to the tail"
        );
    }

    #[test]
    fn move_to_index_same_position_is_noop() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.move_to_index(ZoneId(2), 1));
        assert_eq!(ordered_ids(&zl), vec![1, 2, 3]);
    }

    #[test]
    fn move_to_index_on_single_element_list_is_idempotent() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));

        assert!(zl.move_to_index(ZoneId(1), 0));
        assert!(zl.move_to_index(ZoneId(1), 99));
        assert_eq!(ordered_ids(&zl), vec![1]);
    }

    #[test]
    fn stack_folds_child_under_parent_and_unstack_releases_it() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(matches!(zl.get(ZoneId(1)), Some(parent) if parent.is_stack_anchor()));
        assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.is_stacked_child()));

        assert!(zl.unstack(ZoneId(2)));
        assert!(matches!(zl.get(ZoneId(1)), Some(parent) if !parent.is_stack_anchor()));
        assert!(matches!(zl.get(ZoneId(2)), Some(child) if !child.is_stacked_child()));
    }

    #[test]
    fn unstack_anchor_releases_all_members() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));
        assert!(zl.unstack(ZoneId(1)));

        assert!(matches!(zl.get(ZoneId(1)), Some(parent) if parent.stack_members.is_empty()));
        assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.stack_parent.is_none()));
        assert!(matches!(zl.get(ZoneId(3)), Some(child) if child.stack_parent.is_none()));
    }

    #[test]
    fn dissolve_stack_scattered_releases_members_into_row() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(ZoneId(1), "anchor", 100, 80, 120, 90));
        zl.add(Zone::new(ZoneId(2), "child-a", 100, 80, 120, 90));
        zl.add(Zone::new(ZoneId(3), "child-b", 100, 80, 120, 90));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));
        assert!(zl.dissolve_stack_scattered(ZoneId(1), 640, 480));

        assert!(
            matches!(zl.get(ZoneId(1)), Some(zone) if zone.stack_parent.is_none() && zone.stack_members.is_empty())
        );
        assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.stack_parent.is_none()));
        assert!(matches!(zl.get(ZoneId(3)), Some(zone) if zone.stack_parent.is_none()));
        assert_eq!(
            zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
            Some((100, 80))
        );
        assert_eq!(
            zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
            Some((100 + 120 + STACK_SCATTER_GAP_DIP, 80))
        );
        assert_eq!(
            zl.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
            Some((100 + (120 + STACK_SCATTER_GAP_DIP) * 2, 80))
        );
    }

    #[test]
    fn dissolve_stack_scattered_wraps_and_clamps_near_right_edge() {
        let mut zl = ZoneList::new();
        zl.add(Zone::new(ZoneId(1), "anchor", 250, 40, 120, 80));
        zl.add(Zone::new(ZoneId(2), "child-a", 250, 40, 120, 80));
        zl.add(Zone::new(ZoneId(3), "child-b", 250, 40, 120, 80));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));
        assert!(zl.dissolve_stack_scattered(ZoneId(1), 320, 220));

        assert_eq!(
            zl.get(ZoneId(1)).map(|zone| (zone.x, zone.y)),
            Some((200, 40))
        );
        assert_eq!(
            zl.get(ZoneId(2)).map(|zone| (zone.x, zone.y)),
            Some((200, 40 + 80 + STACK_SCATTER_GAP_DIP))
        );
        assert_eq!(
            zl.get(ZoneId(3)).map(|zone| (zone.x, zone.y)),
            Some((200, 220 - 80))
        );
        for zone in zl.iter() {
            assert!(zone.x >= 0 && zone.y >= 0);
            assert!(zone.x + zone.w <= 320);
            assert!(zone.y + zone.h <= 220);
        }
    }

    #[test]
    fn detach_from_stack_keeps_remaining_members_stacked() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));

        let outcome = zl
            .detach_from_stack(ZoneId(2))
            .expect("member should detach");

        assert_eq!(outcome.detached_member, ZoneId(2));
        assert_eq!(outcome.new_anchor, Some(ZoneId(1)));
        assert_eq!(outcome.remaining_count, 2);
        assert_eq!(zl.stack_anchor_for(ZoneId(3)), Some(ZoneId(1)));
        assert!(matches!(zl.get(ZoneId(2)), Some(child) if child.stack_parent.is_none()));
        assert_eq!(
            zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
            Some(vec![ZoneId(1), ZoneId(3)])
        );
    }

    #[test]
    fn detach_stack_anchor_promotes_remaining_member() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));

        let outcome = zl
            .detach_from_stack(ZoneId(1))
            .expect("anchor should detach");

        assert_eq!(outcome.detached_member, ZoneId(1));
        assert_eq!(outcome.new_anchor, Some(ZoneId(2)));
        assert_eq!(outcome.remaining_count, 2);
        assert!(
            matches!(zl.get(ZoneId(1)), Some(zone) if !zone.is_stacked_child() && !zone.is_stack_anchor())
        );
        assert_eq!(zl.stack_anchor_for(ZoneId(3)), Some(ZoneId(2)));
    }

    #[test]
    fn detach_from_two_member_stack_dissolves_remainder() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));

        let outcome = zl
            .detach_from_stack(ZoneId(2))
            .expect("member should detach");

        assert_eq!(outcome.new_anchor, None);
        assert_eq!(outcome.remaining_count, 1);
        assert!(matches!(zl.get(ZoneId(1)), Some(zone) if !zone.is_stack_anchor()));
        assert!(matches!(zl.get(ZoneId(2)), Some(zone) if !zone.is_stacked_child()));
    }

    #[test]
    fn reorder_stack_member_changes_child_order_under_stable_anchor() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));
        zl.add(zone(4, 40));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));
        assert!(zl.stack(ZoneId(1), ZoneId(3)));
        assert!(zl.stack(ZoneId(1), ZoneId(4)));

        assert!(zl.reorder_stack_member(ZoneId(1), ZoneId(4), 1));

        assert_eq!(
            zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
            Some(vec![ZoneId(1), ZoneId(4), ZoneId(2), ZoneId(3)])
        );
    }

    #[test]
    fn reorder_stack_member_rejects_anchor_and_foreign_member() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));
        zl.add(zone(2, 20));
        zl.add(zone(3, 30));

        assert!(zl.stack(ZoneId(1), ZoneId(2)));

        assert!(!zl.reorder_stack_member(ZoneId(1), ZoneId(1), 1));
        assert!(!zl.reorder_stack_member(ZoneId(1), ZoneId(3), 1));
        assert_eq!(
            zl.stack_member_ids(ZoneId(1)).map(|ids| ids.into_vec()),
            Some(vec![ZoneId(1), ZoneId(2)])
        );
    }

    #[test]
    fn display_name_strips_shortcut_suffix_only() {
        assert_eq!(display_name_for_path("C:/Desktop/App.lnk"), "App");
        assert_eq!(display_name_for_path("C:/Desktop/Site.URL"), "Site");
        assert_eq!(display_name_for_path("C:/Desktop/report.pdf"), "report.pdf");
    }

    #[test]
    fn zone_defaults_include_appearance_fields() {
        let zone = Zone::new(ZoneId(1), Cow::Borrowed("z"), 10, 20, 100, 80);
        assert_eq!(zone.icon.as_ref(), DEFAULT_ZONE_ICON);
        assert_eq!(zone.accent_color.as_deref(), None);
        assert_eq!(zone.grid_columns, DEFAULT_ZONE_GRID_COLUMNS);
        assert_eq!(zone.capsule_size.as_ref(), DEFAULT_ZONE_CAPSULE_SIZE);
        assert_eq!(zone.capsule_shape.as_ref(), DEFAULT_ZONE_CAPSULE_SHAPE);
    }

    #[test]
    fn zone_items_add_move_remove_and_missing_state() {
        let mut zl = ZoneList::new();
        zl.add(zone(1, 10));

        let maybe_item_id = zl.add_item(
            ZoneId(1),
            Cow::Owned("C:/Desktop/report.pdf".to_owned()),
            Cow::Owned("abc".to_owned()),
        );
        assert!(maybe_item_id.is_some());
        let item_id = match maybe_item_id {
            Some(item_id) => item_id,
            None => return,
        };
        assert!(matches!(
            zl.get(ZoneId(1)),
            Some(zone)
                if zone.items.len() == 1
                    && zone.items[0].name.as_ref() == "report.pdf"
                    && zone.items[0].icon_hash.as_ref() == "abc"
        ));

        assert!(zl.move_item(ZoneId(1), item_id, 2, 3));
        assert!(matches!(
            zl.get(ZoneId(1)),
            Some(zone) if zone.items.first().map(|item| (item.x, item.y)) == Some((2, 3))
        ));

        assert!(zl.toggle_item_wide(ZoneId(1), item_id));
        assert!(matches!(
            zl.get(ZoneId(1)),
            Some(zone) if zone.items.first().map(|item| item.is_wide) == Some(true)
        ));

        zl.add(zone(2, 20));
        assert!(zl.move_item_to_zone(ZoneId(1), ZoneId(2), item_id, None, None));
        assert!(matches!(zl.get(ZoneId(1)), Some(zone) if zone.items.is_empty()));
        assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.items.len() == 1));

        assert!(zl.mark_item_missing("C:/Desktop/report.pdf", true));
        assert!(matches!(
            zl.get(ZoneId(2)),
            Some(zone) if zone.items.first().map(|item| item.file_missing) == Some(true)
        ));

        assert!(zl.update_item_file_metadata(
            ZoneId(2),
            item_id,
            Cow::Owned("C:/Desktop/renamed.pdf".to_owned()),
            None,
            Some(Cow::Owned("C:/Original/renamed.pdf".to_owned())),
            Some(Cow::Owned("C:/Desktop/renamed.pdf".to_owned())),
        ));
        assert!(matches!(
            zl.item(ZoneId(2), item_id),
            Some(item)
                if item.name.as_ref() == "renamed.pdf"
                    && item.path.as_ref() == "C:/Desktop/renamed.pdf"
                    && item.original_path.as_deref() == Some("C:/Original/renamed.pdf")
                    && item.hidden_path.as_deref() == Some("C:/Desktop/renamed.pdf")
                    && !item.file_missing
        ));

        assert!(zl.remove_item(ZoneId(2), item_id));
        assert!(matches!(zl.get(ZoneId(2)), Some(zone) if zone.items.is_empty()));
    }

    #[test]
    fn add_item_and_cross_zone_move_use_target_grid_columns() {
        let mut left = zone(1, 10);
        left.set_grid_columns(2);
        let mut right = zone(2, 20);
        right.set_grid_columns(3);
        let mut zl = ZoneList::new();
        zl.add(left);
        zl.add(right);

        let first = zl
            .add_item(
                ZoneId(1),
                Cow::Owned("C:/Desktop/one.txt".to_owned()),
                Cow::Owned("h1".to_owned()),
            )
            .expect("first");
        let second = zl
            .add_item(
                ZoneId(1),
                Cow::Owned("C:/Desktop/two.txt".to_owned()),
                Cow::Owned("h2".to_owned()),
            )
            .expect("second");
        let third = zl
            .add_item(
                ZoneId(1),
                Cow::Owned("C:/Desktop/three.txt".to_owned()),
                Cow::Owned("h3".to_owned()),
            )
            .expect("third");

        assert_eq!(
            zl.item(ZoneId(1), first).map(|item| (item.x, item.y)),
            Some((0, 0))
        );
        assert_eq!(
            zl.item(ZoneId(1), second).map(|item| (item.x, item.y)),
            Some((1, 0))
        );
        assert_eq!(
            zl.item(ZoneId(1), third).map(|item| (item.x, item.y)),
            Some((0, 1))
        );

        assert!(zl.move_item_to_zone(ZoneId(1), ZoneId(2), third, None, None));
        assert_eq!(
            zl.item(ZoneId(2), third).map(|item| (item.x, item.y)),
            Some((0, 0))
        );
    }
}
