#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bentodesk-zone` — zone domain model.
//!
//! A *zone* is BentoDesk's user-defined desktop region — a rectangular area
//! holding shortcuts / widgets / drop targets. This crate defines the pure
//! domain types; persistence lives in `bentodesk-platform::storage` (Ruling
//! 1) and rendering in `bentodesk-app::render` (Ruling 2).
//!
//! Dependency rule: this crate may depend on `bentodesk-style` (Color,
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
/// `String`. The `'static` bound matches `bentodesk-widget::TextNode`.
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
    /// User-facing display alias. When set, all visible Zone title surfaces
    /// prefer this over the canonical title without destroying the title
    /// itself (Tauri's `zone.alias ?? zone.name` contract).
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

fn default_zone_visible() -> bool {
    true
}

mod zone;
mod zone_list;

pub use zone_list::ZoneList;

#[cfg(test)]
mod tests;
