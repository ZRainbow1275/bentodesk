//! Event dispatcher — Phase 1 IPC bus expansion (Wave B / T-013 + T-014).
//!
//! Spec §2 single-process: cross-thread comms via `crossbeam-channel` only.
//! No NamedPipe / socket / IPC server — every "command" rides through this
//! one in-process MPSC bus.
//!
//! Spec §9 NO async runtime: senders are synchronous; the UI thread owns
//! the receiver and drains it once per frame from the WM_PAINT tail.
//!
//! Spec §10 hot-path no alloc: all variant payloads are either `Copy`
//! (`ZoneId`, `Point`, `Size`, `IconHash`) or small-string-optimised
//! (`SmolStr` ≤ 22 inline bytes), and variable-sized payloads use
//! `SmallVec<[T; N]>` so the steady-state dispatch path never heap-allocs.
//!
//! Spec §11 no panic: dispatcher never `panic!`s on a Command. The shell
//! consumer (per Phase 1 scope) uses [`unhandled_command_log`] for variants
//! whose handler hasn't landed yet — debug-only `OutputDebugStringA` write
//! that keeps release-build behaviour as a silent continue.
//!
//! ΔB ruling (master-decomposition §11): every Command variant + every
//! payload struct derives `serde::Serialize + serde::Deserialize`, even
//! though the single-process Phase 1 build never serializes at runtime —
//! preserves the v2.x scripting / plugin re-introduction surface at zero
//! runtime cost.

use core::fmt;

use bentodesk_backend::{grouping::SuggestedGroup, rules::Rule};
use bentodesk_platform::WindowKind;
use bentodesk_zone::ZoneId;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// -----------------------------------------------------------------------------
// Domain payload types — `Copy` where possible, `SmolStr` for small strings,
// `SmallVec` for variable-sized lists. Every type derives serde per ΔB ruling.
// -----------------------------------------------------------------------------

/// 2D point in logical (DIP) screen-space integers. Mirrors the (`i32`, `i32`)
/// shape of `bentodesk-zone::Zone::{x,y}` so move / resize commands round-trip
/// without conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// 2D size in logical (DIP) integers. Same i32 shape as `Zone::{w,h}` so
/// resize commands round-trip without conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0,
        height: 0,
    };

    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

/// Stable per-item identifier inside a zone's item list. `u64` mirrors the
/// `ZoneId` width and lets the future Phase-4 IconPicker route by id without
/// walking the whole list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemId(pub u64);

impl ItemId {
    /// Sentinel reserved for "not yet assigned".
    pub const INVALID: ItemId = ItemId(0);
}

/// Resolved icon hash — the Phase-4 icon cache key. 16 bytes inline keeps
/// the variant small and avoids heap on the dispatch path.
///
/// `[u8; 16]` matches the on-disk icon cache layout (a 128-bit BLAKE3 prefix);
/// `bentodesk-backend` will narrow to whichever digest it ports from 1.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IconHash(pub [u8; 16]);

impl IconHash {
    pub const ZERO: IconHash = IconHash([0u8; 16]);
}

/// Filesystem-rooted item path. `SmolStr` is small-string-optimised
/// (≤ 22 bytes inline); typical Desktop paths fit comfortably in the inline
/// region for short filenames and gracefully heap-allocate for long ones —
/// dispatch path stays alloc-free for the common case (§10).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemPath(pub SmolStr);

impl ItemPath {
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }
}

/// Zone-creation payload. Mirrors the 1.x `create_zone` Tauri command shape
/// (name + initial geometry); the icon / capsule fields default lazily on
/// the consumer side per ΔB forward-compat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneSpec {
    pub name: SmolStr,
    pub origin: Point,
    pub size: Size,
}

/// Tagged setting value — every "set this knob to that" command carries one.
/// Variants cover the four 1.x setting payload shapes; pickers further down
/// the stack pattern-match by key (§17 contract — no `Box<dyn Any>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(SmolStr),
}

/// Backend-bound load-icon request. The reply rides through the
/// [`Request`] / [`RequestSender`] channel so the caller can `recv()` the
/// resulting [`IconHash`] synchronously.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IconRequest {
    pub path: ItemPath,
}

/// One entry in a context-menu's item list. `label` is `SmolStr` (short
/// strings inline); `command_id` is an opaque caller-defined u32 the
/// receiver maps back to a concrete [`Command`] in its own match table.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextMenuItem {
    pub command_id: u32,
    pub label: SmolStr,
}

/// Inline-8 list of [`ContextMenuItem`]s — the shape carried inside
/// [`Command::ShowContextMenu`]'s boxed payload. Typical right-click menus
/// have ≤8 entries so the inline storage avoids heap on the menu-builder
/// path; the surrounding `Box` keeps the Command enum footprint small
/// (`clippy::large_enum_variant`).
pub type ContextMenuItems = smallvec::SmallVec<[ContextMenuItem; 8]>;

/// HWND wrapped as a raw `isize` so the type is `Copy + Send + serde`.
/// `windows::Win32::Foundation::HWND` itself does not derive `Copy` cleanly
/// across the windows / windows-sys split (§3.1.1) and is not `Serialize`,
/// so we erase it to its underlying handle integer at the dispatcher edge.
/// Consumers reconstruct via `windows_sys HWND = bits as *mut _`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowHandle(pub isize);

impl WindowHandle {
    pub const NULL: WindowHandle = WindowHandle(0);
}

/// Target of a palette pick — disambiguates which surface the chosen swatch
/// applies to. `ZoneAccent(id)` updates a single zone's accent colour
/// (1.x `set_zone_accent`); `ThemeBase` rewrites the theme palette anchor
/// (1.x theme CSS-variable application). The selected-stack `ZoneAccent`
/// path emits `SetZoneAccent`; `ThemeBase` emits `SetThemeBase` so the
/// picked swatch becomes visible and persists through the config vault.
///
/// `Copy` because the payload is a discriminator + an `Option<ZoneId>` and
/// rides through the dispatcher's §10 alloc-free hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaletteTarget {
    /// Apply the picked swatch to a specific zone's accent colour.
    ZoneAccent(ZoneId),
    /// Apply the picked swatch as the theme base colour (process-wide).
    ThemeBase,
    /// Apply the picked swatch to the currently selected BulkManager zones.
    BulkManagerSelectedAccent,
}

/// Tauri-compatible bulk auto-layout algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BulkLayoutAlgorithm {
    #[default]
    Grid,
    Row,
    Column,
    Spiral,
    Organic,
}

impl BulkLayoutAlgorithm {
    pub const ALL: &'static [Self] = &[
        Self::Grid,
        Self::Row,
        Self::Column,
        Self::Spiral,
        Self::Organic,
    ];

    pub const fn wire(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Row => "row",
            Self::Column => "column",
            Self::Spiral => "spiral",
            Self::Organic => "organic",
        }
    }

    pub fn parse(token: &str) -> Self {
        match token {
            "grid" => Self::Grid,
            "row" => Self::Row,
            "column" => Self::Column,
            "spiral" => Self::Spiral,
            "organic" => Self::Organic,
            _ => Self::default(),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::Row => "Row",
            Self::Column => "Column",
            Self::Spiral => "Spiral",
            Self::Organic => "Organic",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Grid => "Snap selected zones to a uniform grid.",
            Self::Row => "Arrange selected zones in a horizontal row.",
            Self::Column => "Arrange selected zones in a vertical column.",
            Self::Spiral => "Place selected zones along a deterministic spiral.",
            Self::Organic => "Pack selected zones with organic repulsion.",
        }
    }

    pub const fn icon_slug(self) -> &'static str {
        match self {
            Self::Grid => "grid-3x3",
            Self::Row => "rows-3",
            Self::Column => "columns-3",
            Self::Spiral => "rotate-ccw",
            Self::Organic => "sparkles",
        }
    }
}

impl fmt::Display for BulkLayoutAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Partial update payload for one zone inside a bulk operation.
///
/// Mirrors the 1.x `BulkZoneUpdate` wire semantics while adapting geometry to
/// selected-stack logical coordinates: every `None` field leaves the current
/// zone state untouched, `alias = Some("")` clears the alias,
/// `accent_color = Some("")` clears the accent, `display_mode = Some(None)`
/// clears the override, and an empty `icon` is a no-op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkZoneUpdate {
    /// Stable target zone id. Unknown/stale ids are skipped by the shell.
    pub id: ZoneId,
    /// Optional new top-left position in logical DIPs.
    #[serde(default)]
    pub position: Option<Point>,
    /// Optional new expanded size in logical DIPs.
    #[serde(default)]
    pub size: Option<Size>,
    /// Optional accent colour. `None` means unchanged; empty string clears the
    /// accent override.
    #[serde(default)]
    pub accent_color: Option<SmolStr>,
    /// Optional capsule size token (`small`, `medium`, `large`).
    #[serde(default)]
    pub capsule_size: Option<SmolStr>,
    /// Optional lock flag.
    #[serde(default)]
    pub locked: Option<bool>,
    /// Optional alias write. Whitespace-only values clear the alias.
    #[serde(default)]
    pub alias: Option<SmolStr>,
    /// Optional display mode write. `Some(None)` clears to inherited mode.
    #[serde(default)]
    pub display_mode: Option<Option<SmolStr>>,
    /// Optional icon slug. Whitespace-only values are ignored.
    #[serde(default)]
    pub icon: Option<SmolStr>,
}

impl Default for BulkZoneUpdate {
    fn default() -> Self {
        Self {
            id: ZoneId::INVALID,
            position: None,
            size: None,
            accent_color: None,
            capsule_size: None,
            locked: None,
            alias: None,
            display_mode: None,
            icon: None,
        }
    }
}

mod channel;
mod command;

pub use channel::*;
pub use command::*;

#[cfg(test)]
mod tests;
