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

use bento_nano_backend::{grouping::SuggestedGroup, rules::Rule};
use bento_nano_platform::WindowKind;
use bento_nano_zone::ZoneId;
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// -----------------------------------------------------------------------------
// Domain payload types — `Copy` where possible, `SmolStr` for small strings,
// `SmallVec` for variable-sized lists. Every type derives serde per ΔB ruling.
// -----------------------------------------------------------------------------

/// 2D point in logical (DIP) screen-space integers. Mirrors the (`i32`, `i32`)
/// shape of `bento-nano-zone::Zone::{x,y}` so move / resize commands round-trip
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
/// `bento-nano-backend` will narrow to whichever digest it ports from 1.x.
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

// -----------------------------------------------------------------------------
// Command — the closed enum every UI/background producer dispatches into.
// Phase 1 expansion: ~30 variants spanning Zone CRUD, Item CRUD, Settings,
// Icon ops, MiniBar, Tooltip, ContextMenu, Window lifecycle.
// -----------------------------------------------------------------------------

/// Application command — one of these is dispatched per UI event.
///
/// Closed enum (no `Box<dyn Any>`) so the consumer's match in
/// `bento-nano-shell::main::consume_dispatcher` (or the per-window wndproc
/// equivalent) is exhaustive at compile time. ΔB ruling: serde-derived even
/// though never serialized at runtime in Phase 1.
///
/// Variant ordering is grouped by domain (Toolbar / Window / Zone CRUD /
/// Item CRUD / Settings / Icon / MiniBar / Tooltip / ContextMenu / Lifecycle)
/// — see the rustdoc next to each variant for the originating 1.x command
/// or new Phase-1+ surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    // -------- Toolbar (Phase 2.1 baseline; kept for backwards-compat) --------
    /// Toolbar PIN button — toggle always-on-top state.
    TogglePin,
    /// Toolbar SETTINGS button — flip the inline §C settings overlay.
    ToggleSettings,
    /// Settings panel close button or click-outside (§C).
    CloseSettings,
    /// Settings panel locale-switch button — flip zh-CN ⇄ en-US (§C).
    ToggleLocale,
    /// Tray icon right-click — open the tray popup menu.
    ShowTrayMenu,

    // -------- Window lifecycle (Phase 1 multi-window expansion) --------
    /// Bring `kind` on-screen — `ShowWindow(SW_SHOW) + SetForegroundWindow`.
    ShowWindow(WindowKind),
    /// Hide `kind` — `ShowWindow(SW_HIDE)`. Tray icon survives so the user
    /// can revive the main window via tray left-click.
    HideWindow(WindowKind),

    // -------- Zone CRUD (mirrors 1.x src-tauri/src/commands/zone.rs) --------
    /// Create a new zone with the given spec.
    CreateZone(ZoneSpec),
    /// Remove the zone with the given id.
    DeleteZone(ZoneId),
    /// Rename a zone (1.x `update_zone { name: Some(_) }`).
    RenameZone(ZoneId, SmolStr),
    /// Move a zone to a new origin (1.x `update_zone { position: Some(_) }`).
    MoveZone(ZoneId, Point),
    /// Resize a zone (1.x `update_zone { expanded_size: Some(_) }`).
    ResizeZone(ZoneId, Size),
    /// Group two zones into a stack (1.x `stack_zones`). Payload: (anchor,
    /// new_member). Multi-zone stacking is a sequence of `StackZone` calls.
    StackZone(ZoneId, ZoneId),
    /// Dissolve the stack a zone belongs to (1.x `unstack_zones`).
    UnstackZone(ZoneId),
    /// Open the selected-stack StackTray overlay for a stack anchor or member.
    OpenStackTray(ZoneId),
    /// Close the StackTray overlay without mutating stack membership.
    CloseStackTray,
    /// Select a real stack member for the FocusedZonePreview overlay.
    PreviewStackMember(ZoneId, ZoneId),
    /// Detach one member from a real stack and keep any remaining stack valid.
    DetachStackMember(ZoneId, ZoneId),
    /// Dissolve every member from the stack containing the given zone.
    DissolveStack(ZoneId),
    /// Move a child stack member to a new visible StackTray row. The anchor
    /// remains stable; target index is measured against the full visible
    /// member list where row 0 is the anchor.
    ReorderStackMember(ZoneId, ZoneId, usize),
    /// Set or clear a zone's display alias (1.x `set_zone_alias`).
    SetZoneAlias(ZoneId, SmolStr),
    /// Update a zone's icon slug (selected-stack follow-up for icon picker /
    /// ZoneEditor preset cycle).
    SetZoneIcon(ZoneId, SmolStr),
    /// Update or clear a zone's accent swatch.
    SetZoneAccent(ZoneId, Option<SmolStr>),
    /// Update or clear the process-wide theme base accent swatch.
    SetThemeBase(Option<SmolStr>),
    /// Select a full JSON/built-in theme by id after shell-side validation.
    SetActiveTheme(SmolStr),
    /// Import a user-selected JSON theme file from a real filesystem path.
    ImportTheme(SmolStr),
    /// Refresh installed plugins from the real selected-stack plugin registry.
    ListPlugins,
    /// Install a user-selected `.bdplugin`/zip archive through the selected-stack
    /// safe extraction and registry persistence path.
    InstallPlugin(SmolStr),
    /// Enable or disable an installed plugin by id.
    TogglePlugin(SmolStr, bool),
    /// Uninstall an installed plugin by id.
    UninstallPlugin(SmolStr),
    /// Update the per-zone item-grid column count.
    SetZoneGridColumns(ZoneId, u32),
    /// Update the per-zone capsule appearance pair `(size, shape)`.
    SetZoneCapsule(ZoneId, SmolStr, SmolStr),
    /// Open the native selected-stack folder picker before binding a live folder.
    OpenLiveFolderPicker(ZoneId),
    /// Bind a zone to a user folder as a read-only live mirror (1.x
    /// `bind_zone_to_folder`).
    BindZoneToFolder(ZoneId, SmolStr),
    /// Remove the read-only live-folder mirror from a zone (1.x
    /// `unbind_zone_folder`).
    UnbindZoneFolder(ZoneId),
    /// Re-scan a bound live folder into its zone item list (1.x
    /// `scan_live_folder` + `zone_live_refresh` consumer).
    RefreshLiveFolder(ZoneId),
    /// Move a zone to a new sort_order slot (1.x `reorder_zones`).
    ReorderZone(ZoneId, u32),
    /// Duplicate the current selected zone or first visible zone.
    DuplicateZone,
    /// Toggle the current selected zone's locked flag.
    ToggleSelectedZoneLock,
    /// Hide all visible zones or show all zones when none are visible.
    ToggleAllZonesVisible,
    /// Apply the selected-stack grid reflow to visible top-level zones.
    ReflowVisibleZones,
    /// Focus the next visible top-level zone.
    FocusNextZone,
    /// Focus the previous visible top-level zone.
    FocusPreviousZone,

    // -------- Item CRUD (mirrors 1.x src-tauri/src/commands/item.rs) --------
    /// Add an item to a zone (1.x `add_item`).
    AddItem(ZoneId, ItemPath),
    /// Remove an item from a zone (1.x `remove_item`).
    RemoveItem(ZoneId, ItemId),
    /// Open an item's effective filesystem path (1.x MiniBar
    /// `minibar-launch-item` forwarder / item context `Open`).
    OpenItemFile(ZoneId, ItemId),
    /// Copy an item's effective filesystem path to the Windows clipboard
    /// (1.x item context menu `Copy Path`).
    CopyItemPath(ItemPath),
    /// Move an item within or between zones (1.x `move_item`). The
    /// destination zone is implicit in the consumer's hit-test result.
    MoveItem(ZoneId, ItemId, Point),
    /// Toggle a card between one-column and two-column width (1.x
    /// `toggle_item_wide`).
    ToggleItemWide(ZoneId, ItemId),
    /// Move an item to another zone (1.x `move_item { from_zone_id,
    /// to_zone_id, item_id }`).
    MoveItemToZone(ZoneId, ZoneId, ItemId),
    /// Open the selected-stack native file rename surface for an item.
    OpenItemFileRename(ZoneId, ItemId),
    /// Rename the item's real filesystem entry in-place.
    RenameItemFile(ZoneId, ItemId, SmolStr),
    /// Move the item's real filesystem entry to the Windows recycle bin.
    DeleteItemFileToRecycleBin(ZoneId, ItemId),

    // -------- Settings (mirrors 1.x src-tauri/src/commands/settings.rs) --------
    /// Open the modal settings panel (1.x main-menu entry).
    OpenSettings,
    /// Open the About surface from tray/menu routing. This is a real
    /// selected-stack command, not a log-only placeholder: the shell toggles
    /// `AppState::about_open`, mounts `business::about::build()`, and shows
    /// the `WindowKind::About` HWND.
    OpenAbout,
    /// Close the About surface. Produced by the About close button or by
    /// click-outside hit testing on the selected-stack overlay.
    CloseAbout,
    /// Toggle the selected-stack diagnostics HUD. Mirrors the 1.x
    /// `settings.debug_overlay` diagnostic surface with a runtime command
    /// so Search/actions and restored settings can reach the D2D overlay.
    ToggleDebugOverlay,
    /// Set a single setting value (1.x `update_settings` per-field path).
    SetSetting { key: SmolStr, value: SettingValue },
    /// Reset one supported `keybinding.<action>` override back to its
    /// selected-stack default by removing the persisted override and updating
    /// the in-process hotkey table.
    ResetKeybinding { action: SmolStr },
    /// Create a real config-vault backup file from the Settings panel.
    CreateSettingsBackup,
    /// Enumerate real config-vault backup files into visible Settings state.
    ListSettingsBackups,
    /// Restore the newest real config-vault backup and re-apply persisted
    /// runtime settings.
    RestoreLatestSettingsBackup,
    /// Restore a specific real config-vault backup by stable backup id. The
    /// shell resolves this id by re-enumerating `backups/vault-*.bin`, never
    /// by trusting a UI-supplied filesystem path.
    RestoreSettingsBackup(SmolStr),
    /// Capture a synchronized selected-stack recovery bundle containing the
    /// current `zones.bin` payload. Mirrors 1.x `recovery_bundle::refresh_from_state`.
    CreateRecoveryBundle,
    /// Export a validated recovery diagnostics JSON beside the current
    /// selected-stack recovery bundle.
    ExportRecoveryDiagnostics,
    /// Restore the current selected-stack layout from the latest validated
    /// recovery bundle. The shell validates the bundle checksum before
    /// replacing live `ZoneList` state.
    RestoreRecoveryBundle,
    /// Set the config vault to passphrase encryption using the passphrase
    /// captured by the selected-stack Settings keyboard input path.
    SetEncryptionPassphrase(SmolStr),
    /// Unlock an already passphrase-encrypted config vault on cold start.
    UnlockEncryptionPassphrase(SmolStr),
    /// Check the updater endpoint through the selected-stack backend updater
    /// surface (1.x `check_for_updates`).
    CheckForUpdates,
    /// Download the pending update through the selected-stack backend updater
    /// surface (1.x `download_update`).
    DownloadUpdate,
    /// Install the staged update and restart (1.x `install_update_and_restart`).
    InstallUpdateAndRestart,
    /// Persist a user-skipped update version (1.x `skip_update_version`).
    SkipUpdateVersion(SmolStr),

    // -------- Desktop organization (mirrors 1.x tray_auto_organize) --------
    /// Scan real Desktop sources, ask the backend grouping engine for
    /// suggestions, and apply accepted groups into the zone model.
    AutoOrganize,
    /// Open the native selected-stack SearchBar as a `WindowKind::Search`
    /// aux HWND. The shell seeds it from live zones/items/settings/actions.
    OpenSearch,
    /// Query the current SearchBar index. Payload is the user-typed needle.
    QuerySearch(SmolStr),
    /// Activate a visible SearchBar result by stable hit id.
    ActivateSearchResult(SmolStr),
    /// Close the Search aux HWND without activating a result.
    CloseSearch,

    // -------- Icon ops (mirrors 1.x src-tauri/src/commands/icon.rs) --------
    /// Trigger a backend-side icon extraction/cache warm for a concrete
    /// desktop path. The selected-stack shell calls
    /// `bento_nano_backend::icon::protocol::extract_and_cache` directly.
    LoadIcon(ItemPath),

    // -------- Picker openers (Phase 2 dialog spawn — F2-07) --------
    /// Open the icon picker as a `WindowKind::IconPicker` aux HWND.
    /// `zone_id` is the target zone whose icon the user is picking; `None`
    /// is the non-zone BulkManager path and applies the selected icon to the
    /// currently selected BulkManager zone rows.
    OpenIconPicker { zone_id: Option<ZoneId> },
    /// Open the palette popover as a `WindowKind::PalettePicker` aux HWND.
    /// `target` discriminates whether the picked swatch applies to a zone
    /// accent, the theme base, or the current BulkManager selection. Zone
    /// accent selection emits `SetZoneAccent`; theme-base selection emits
    /// `SetThemeBase`; BulkManager selection emits bulk metadata commands.
    OpenPalettePicker { target: PaletteTarget },
    /// Open the Context Capsule browser/capture/restore modal as a
    /// `WindowKind::CapsulePicker` aux HWND.
    OpenCapsulePicker,
    /// Capture the current selected-stack zone layout as a filesystem-backed
    /// Context Capsule.
    CaptureCapsule(SmolStr),
    /// Restore a previously captured Context Capsule by stable id.
    RestoreCapsule(SmolStr),
    /// Delete a previously captured Context Capsule by stable id.
    DeleteCapsule(SmolStr),

    // -------- Timeline / recovery (mirrors 1.x src-tauri/src/commands/timeline.rs) --------
    /// Open the selected-stack Timeline panel and load checkpoint metadata
    /// from the real `<state_dir>/timeline` store.
    OpenTimeline,
    /// Save a manual checkpoint or pin an existing checkpoint. `id = None`
    /// creates a new pinned checkpoint from the live selected-stack layout;
    /// `Some(id)` promotes the existing checkpoint to pinned.
    SaveCheckpoint {
        id: Option<SmolStr>,
        label: Option<SmolStr>,
    },
    /// Restore the live selected-stack layout to a concrete checkpoint id.
    RestoreCheckpoint(SmolStr),
    /// Restore the previous checkpoint relative to the timeline cursor.
    UndoCheckpoint,
    /// Restore the next checkpoint relative to the timeline cursor.
    RedoCheckpoint,
    /// Delete a checkpoint by stable visible id.
    DeleteCheckpoint(SmolStr),
    /// Open the layout SnapshotPicker and load saved snapshot metadata from
    /// the real `<state_dir>/snapshots` store.
    OpenSnapshotPicker,
    /// Save the current selected-stack layout as a named snapshot.
    SaveSnapshot { name: Option<SmolStr> },
    /// Load a saved layout snapshot by stable visible id.
    LoadSnapshot(SmolStr),
    /// Delete a saved layout snapshot by stable visible id.
    DeleteSnapshot(SmolStr),

    // -------- Wizard / overlay openers (Phase 2 modal spawn — F2-08) --------
    /// Open the multi-step rules wizard as a `WindowKind::RulesWizard` aux
    /// HWND (`business::rules_wizard`). Save / preview / delete are handled
    /// by the selected-stack rules command variants below.
    OpenRulesWizard,
    /// Create or update a rule from the RulesWizard. Empty `Rule::id` means
    /// create; the shell stamps a stable id before calling `rules::upsert`.
    SaveRule(Box<Rule>),
    /// Delete a persisted rule by stable id. The shell resolves only ids that
    /// came from the visible `rules.json` list.
    DeleteRule(SmolStr),
    /// Compute preview hits for the wizard without applying rule actions.
    PreviewRuleHits(Box<Rule>),
    /// Execute a persisted rule by stable id and surface the resulting
    /// `ExecutionReport` in the RulesWizard status area.
    RunRuleNow(SmolStr),
    /// Open the bulk-action manager panel as a `WindowKind::BulkManager`
    /// aux HWND (`business::bulk_manager_panel`). Selection rides follow-up
    /// F3/F5 bulk-update Commands (deferred until those waves wire it).
    OpenBulkManager,
    /// Delete multiple zones as one BulkManager action. The shell applies
    /// existing per-zone delete semantics for each id and saves once.
    BulkDeleteZones(Vec<ZoneId>),
    /// Hide or show multiple zones without deleting their persisted layout.
    /// Hidden zones remain visible in BulkManager rows so the same surface can
    /// restore them.
    BulkSetZonesVisible { ids: Vec<ZoneId>, visible: bool },
    /// Apply a Tauri-compatible auto-layout algorithm to selected/listed zones.
    BulkApplyLayout {
        ids: Vec<ZoneId>,
        algorithm: BulkLayoutAlgorithm,
    },
    /// Apply 1.x-compatible partial metadata/geometry updates to many zones.
    BulkUpdateZones(Vec<BulkZoneUpdate>),
    /// Move multiple zones by a shared delta as one BulkManager action.
    BulkMoveZones { ids: Vec<ZoneId>, delta: Point },
    /// Open the per-zone form editor as a `WindowKind::ZoneEditor` aux
    /// HWND (`business::zone_editor`). `zone_id` is the editing target.
    /// The selected-stack keyboard fallback now edits the draft inline and
    /// dispatches `RenameZone` plus the zone-appearance Commands on Enter;
    /// direct pointer controls inside the surface remain follow-up work.
    OpenZoneEditor(ZoneId),
    /// Show the smart-group suggestor panel as a `WindowKind::Suggestor`
    /// aux HWND (`business::smart_group_suggestor`). Pairs with the
    /// existing `SuggestorDismiss` (per-row dismiss) and `GroupingApply`
    /// (per-row apply) Commands.
    ShowSuggestor,

    // -------- MiniBar (mirrors 1.x src-tauri/src/commands/minibar.rs) --------
    /// Pin a zone as a floating minibar (1.x `pin_zone_as_minibar`).
    PinZoneAsMinibar(ZoneId),
    /// Unpin a minibar back into its parent zone (1.x `unpin_minibar`).
    UnpinMinibar(ZoneId),
    /// List currently pinned minibars (1.x `list_pinned_minibars`).
    ListPinnedMinibars,

    // -------- Tooltip (Phase 2 hover surface) --------
    /// Anchor + message for a hover tooltip. `anchor` is the HWND the
    /// tooltip should attach to (caret follows `anchor`'s client rect).
    ShowTooltip { anchor: WindowHandle, text: SmolStr },
    /// Dismiss the active tooltip (idempotent).
    HideTooltip,

    // -------- SmartGroupSuggestor (T-069a, Phase 4 AI grouping surface) --------
    /// User clicked **Apply** on a suggested grouping. The shell forwards
    /// the payload to `bento_nano_backend::grouping::apply_auto_group`
    /// (suggestion's `rule` + `matching_files`). Box keeps the variant
    /// footprint small enough for `clippy::large_enum_variant` —
    /// `SuggestedGroup` carries up to MAX_CLUSTER_SIZE (15) `String` paths
    /// so the inline payload would dominate the enum size; this is a
    /// click-frequency command (not §10 hot-path), so the one heap-alloc
    /// per Apply is acceptable.
    GroupingApply { suggestion: Box<SuggestedGroup> },
    /// User clicked the row dismiss / close affordance for a single
    /// suggestion. The shell removes it from the panel's local state — no
    /// backend round-trip required.
    SuggestorDismiss { suggestion_id: SmolStr },

    // -------- ContextMenu (Phase 2 right-click surface) --------
    /// Show a right-click context menu at `anchor` with the given items.
    /// `Box<ContextMenuItems>` keeps the variant footprint small enough
    /// (8 bytes for the pointer) to satisfy `clippy::large_enum_variant`
    /// — every other Command variant costs only a few words to memcpy on
    /// the §10 hot-path send path. ShowContextMenu is a low-frequency
    /// (right-click) command, so the one heap-alloc per menu open is
    /// acceptable; the 280-byte memcpy tax it would otherwise impose on
    /// every TogglePin / MouseMove dispatch is not.
    ShowContextMenu {
        anchor: WindowHandle,
        items: Box<ContextMenuItems>,
    },
    /// Dismiss the active context menu (idempotent).
    HideContextMenu,

    // -------- Lifecycle (Phase 2.1 baseline; kept for backwards-compat) --------
    /// Tray menu「退出」or any other path that wants the message loop to
    /// exit. Consumer calls `PostQuitMessage(0)`.
    QuitApp,
}

impl Command {
    /// Static, allocation-free variant name (matches the source variant
    /// identifier verbatim). Used by [`unhandled_command_log`] so the
    /// debug-only diagnostic stays §10 hot-path safe.
    pub const fn variant_name(&self) -> &'static str {
        match self {
            Self::TogglePin => "TogglePin",
            Self::ToggleSettings => "ToggleSettings",
            Self::CloseSettings => "CloseSettings",
            Self::ToggleLocale => "ToggleLocale",
            Self::ShowTrayMenu => "ShowTrayMenu",
            Self::ShowWindow(_) => "ShowWindow",
            Self::HideWindow(_) => "HideWindow",
            Self::CreateZone(_) => "CreateZone",
            Self::DeleteZone(_) => "DeleteZone",
            Self::RenameZone(_, _) => "RenameZone",
            Self::MoveZone(_, _) => "MoveZone",
            Self::ResizeZone(_, _) => "ResizeZone",
            Self::StackZone(_, _) => "StackZone",
            Self::UnstackZone(_) => "UnstackZone",
            Self::OpenStackTray(_) => "OpenStackTray",
            Self::CloseStackTray => "CloseStackTray",
            Self::PreviewStackMember(_, _) => "PreviewStackMember",
            Self::DetachStackMember(_, _) => "DetachStackMember",
            Self::DissolveStack(_) => "DissolveStack",
            Self::ReorderStackMember(_, _, _) => "ReorderStackMember",
            Self::SetZoneAlias(_, _) => "SetZoneAlias",
            Self::SetZoneIcon(_, _) => "SetZoneIcon",
            Self::SetZoneAccent(_, _) => "SetZoneAccent",
            Self::SetThemeBase(_) => "SetThemeBase",
            Self::SetActiveTheme(_) => "SetActiveTheme",
            Self::ImportTheme(_) => "ImportTheme",
            Self::ListPlugins => "ListPlugins",
            Self::InstallPlugin(_) => "InstallPlugin",
            Self::TogglePlugin(_, _) => "TogglePlugin",
            Self::UninstallPlugin(_) => "UninstallPlugin",
            Self::SetZoneGridColumns(_, _) => "SetZoneGridColumns",
            Self::SetZoneCapsule(_, _, _) => "SetZoneCapsule",
            Self::OpenLiveFolderPicker(_) => "OpenLiveFolderPicker",
            Self::BindZoneToFolder(_, _) => "BindZoneToFolder",
            Self::UnbindZoneFolder(_) => "UnbindZoneFolder",
            Self::RefreshLiveFolder(_) => "RefreshLiveFolder",
            Self::ReorderZone(_, _) => "ReorderZone",
            Self::DuplicateZone => "DuplicateZone",
            Self::ToggleSelectedZoneLock => "ToggleSelectedZoneLock",
            Self::ToggleAllZonesVisible => "ToggleAllZonesVisible",
            Self::ReflowVisibleZones => "ReflowVisibleZones",
            Self::FocusNextZone => "FocusNextZone",
            Self::FocusPreviousZone => "FocusPreviousZone",
            Self::AddItem(_, _) => "AddItem",
            Self::RemoveItem(_, _) => "RemoveItem",
            Self::OpenItemFile(_, _) => "OpenItemFile",
            Self::CopyItemPath(_) => "CopyItemPath",
            Self::MoveItem(_, _, _) => "MoveItem",
            Self::ToggleItemWide(_, _) => "ToggleItemWide",
            Self::MoveItemToZone(_, _, _) => "MoveItemToZone",
            Self::OpenItemFileRename(_, _) => "OpenItemFileRename",
            Self::RenameItemFile(_, _, _) => "RenameItemFile",
            Self::DeleteItemFileToRecycleBin(_, _) => "DeleteItemFileToRecycleBin",
            Self::OpenSettings => "OpenSettings",
            Self::OpenAbout => "OpenAbout",
            Self::CloseAbout => "CloseAbout",
            Self::ToggleDebugOverlay => "ToggleDebugOverlay",
            Self::SetSetting { .. } => "SetSetting",
            Self::ResetKeybinding { .. } => "ResetKeybinding",
            Self::CreateSettingsBackup => "CreateSettingsBackup",
            Self::ListSettingsBackups => "ListSettingsBackups",
            Self::RestoreLatestSettingsBackup => "RestoreLatestSettingsBackup",
            Self::RestoreSettingsBackup(_) => "RestoreSettingsBackup",
            Self::CreateRecoveryBundle => "CreateRecoveryBundle",
            Self::ExportRecoveryDiagnostics => "ExportRecoveryDiagnostics",
            Self::RestoreRecoveryBundle => "RestoreRecoveryBundle",
            Self::SetEncryptionPassphrase(_) => "SetEncryptionPassphrase",
            Self::UnlockEncryptionPassphrase(_) => "UnlockEncryptionPassphrase",
            Self::CheckForUpdates => "CheckForUpdates",
            Self::DownloadUpdate => "DownloadUpdate",
            Self::InstallUpdateAndRestart => "InstallUpdateAndRestart",
            Self::SkipUpdateVersion(_) => "SkipUpdateVersion",
            Self::AutoOrganize => "AutoOrganize",
            Self::OpenSearch => "OpenSearch",
            Self::QuerySearch(_) => "QuerySearch",
            Self::ActivateSearchResult(_) => "ActivateSearchResult",
            Self::CloseSearch => "CloseSearch",
            Self::LoadIcon(_) => "LoadIcon",
            Self::OpenIconPicker { .. } => "OpenIconPicker",
            Self::OpenPalettePicker { .. } => "OpenPalettePicker",
            Self::OpenCapsulePicker => "OpenCapsulePicker",
            Self::CaptureCapsule(_) => "CaptureCapsule",
            Self::RestoreCapsule(_) => "RestoreCapsule",
            Self::DeleteCapsule(_) => "DeleteCapsule",
            Self::OpenTimeline => "OpenTimeline",
            Self::SaveCheckpoint { .. } => "SaveCheckpoint",
            Self::RestoreCheckpoint(_) => "RestoreCheckpoint",
            Self::UndoCheckpoint => "UndoCheckpoint",
            Self::RedoCheckpoint => "RedoCheckpoint",
            Self::DeleteCheckpoint(_) => "DeleteCheckpoint",
            Self::OpenSnapshotPicker => "OpenSnapshotPicker",
            Self::SaveSnapshot { .. } => "SaveSnapshot",
            Self::LoadSnapshot(_) => "LoadSnapshot",
            Self::DeleteSnapshot(_) => "DeleteSnapshot",
            Self::OpenRulesWizard => "OpenRulesWizard",
            Self::SaveRule(_) => "SaveRule",
            Self::DeleteRule(_) => "DeleteRule",
            Self::PreviewRuleHits(_) => "PreviewRuleHits",
            Self::RunRuleNow(_) => "RunRuleNow",
            Self::OpenBulkManager => "OpenBulkManager",
            Self::BulkDeleteZones(_) => "BulkDeleteZones",
            Self::BulkSetZonesVisible { .. } => "BulkSetZonesVisible",
            Self::BulkApplyLayout { .. } => "BulkApplyLayout",
            Self::BulkUpdateZones(_) => "BulkUpdateZones",
            Self::BulkMoveZones { .. } => "BulkMoveZones",
            Self::OpenZoneEditor(_) => "OpenZoneEditor",
            Self::ShowSuggestor => "ShowSuggestor",
            Self::PinZoneAsMinibar(_) => "PinZoneAsMinibar",
            Self::UnpinMinibar(_) => "UnpinMinibar",
            Self::ListPinnedMinibars => "ListPinnedMinibars",
            Self::ShowTooltip { .. } => "ShowTooltip",
            Self::HideTooltip => "HideTooltip",
            Self::GroupingApply { .. } => "GroupingApply",
            Self::SuggestorDismiss { .. } => "SuggestorDismiss",
            Self::ShowContextMenu { .. } => "ShowContextMenu",
            Self::HideContextMenu => "HideContextMenu",
            Self::QuitApp => "QuitApp",
        }
    }
}

/// Spec §11 no-panic helper — log an unhandled `Command` variant via
/// `OutputDebugStringA` in **debug builds only**, then continue. The
/// consumer's exhaustive match should call this from its `_ =>` arm for
/// any variant whose handler hasn't landed yet (Phase-1 partial).
///
/// Release builds compile to a no-op so there is zero runtime cost in
/// production. Debug builds route to the Win32 system-wide debug stream
/// which DebugView monitors — same channel `bento-nano-platform::allocator`
/// uses for pre-main probe diagnostics.
#[inline]
pub fn unhandled_command_log(cmd: &Command) {
    #[cfg(debug_assertions)]
    {
        // Stack-allocated 96-byte buffer fits the prefix + the longest
        // variant name + the NUL terminator with headroom. SmallVec keeps
        // the buffer off the heap on the §10 hot path.
        const PREFIX: &[u8] = b"BentoDesk: unhandled command variant ";
        let name = cmd.variant_name().as_bytes();
        // 38 (prefix) + 32 (max variant name budget) + 1 (NUL) = 71 < 96.
        let mut buf: smallvec::SmallVec<[u8; 96]> = smallvec::SmallVec::new();
        buf.extend_from_slice(PREFIX);
        buf.extend_from_slice(name);
        buf.push(0); // NUL-terminate for OutputDebugStringA's PCSTR contract.
        // SAFETY: buf is NUL-terminated above; OutputDebugStringA reads
        //         until NUL and does not retain the pointer.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringA(buf.as_ptr());
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = cmd;
    }
}

// -----------------------------------------------------------------------------
// MPSC dispatcher (kept binary-compatible with Phase 2.1 consumers).
// -----------------------------------------------------------------------------

/// Type-aliased `Sender<Command>` — exposed for producers that want a bare
/// crossbeam handle (background workers wired up via `EventDispatcher::sender`
/// already get a `Sender<Command>`; this alias is for crates that import the
/// type directly).
pub type CommandSender = Sender<Command>;

/// Type-aliased `Receiver<Command>` — symmetric with [`CommandSender`].
pub type CommandReceiver = Receiver<Command>;

/// Hand-rolled (no thiserror — §8.1) error returned from dispatcher send /
/// recv operations when the channel partner has been dropped. Phase 1
/// callers that use the bus rarely care about this case (the receiver is
/// held by the wndproc for the life of the process); it exists so the
/// dispatcher's public surface composes into spec §11's `Result`-only
/// no-panic discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherError {
    /// The matched receiver has been dropped — the message will never
    /// reach a consumer.
    ReceiverDisconnected,
    /// The matched sender has been dropped — no further messages will
    /// arrive on the receiver.
    SenderDisconnected,
}

impl fmt::Display for DispatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReceiverDisconnected => f.write_str("dispatcher receiver disconnected"),
            Self::SenderDisconnected => f.write_str("dispatcher sender disconnected"),
        }
    }
}

impl core::error::Error for DispatcherError {}

/// MPSC dispatcher. Multiple producers via cloned [`CommandSender`]s; single
/// consumer drains via [`EventDispatcher::drain_into`].
#[derive(Debug, Clone)]
pub struct EventDispatcher {
    tx: CommandSender,
    rx: CommandReceiver,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDispatcher {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self { tx, rx }
    }

    /// Get a clonable producer handle for use on background threads.
    pub fn sender(&self) -> CommandSender {
        self.tx.clone()
    }

    /// Push a command synchronously from the UI thread. Returns `false`
    /// when the receiver has been dropped (Phase-1 contract: this only
    /// happens during shutdown after WM_DESTROY).
    pub fn push(&self, cmd: Command) -> bool {
        self.tx.send(cmd).is_ok()
    }

    /// Drain all pending commands into `out`. Returns the number drained.
    pub fn drain_into(&self, out: &mut smallvec::SmallVec<[Command; 8]>) -> usize {
        let mut count = 0;
        while let Ok(c) = self.rx.try_recv() {
            out.push(c);
            count += 1;
        }
        count
    }
}

// -----------------------------------------------------------------------------
// T-014 — Request/reply channel for backend ops (LoadIcon, etc.)
// -----------------------------------------------------------------------------

/// One pending request — pairs a `req` with the one-shot reply channel the
/// caller blocks on. The backend handler `recv()`s the request, computes
/// the response, and `send()`s it back through `reply`.
///
/// Single-thread invariant (spec §9): the backend handler runs on its own
/// `std::thread` worker (T-100 future task) consuming `Request<Req, Resp>`;
/// the UI thread sends the request and `recv()`s on the bounded(1) reply
/// channel. NO tokio anywhere in this path — crossbeam-channel is the
/// only inter-thread mechanism.
#[derive(Debug)]
pub struct Request<Req, Resp> {
    pub req: Req,
    pub reply: Sender<Resp>,
}

/// Producer half of the request/reply channel.
#[derive(Debug, Clone)]
pub struct RequestSender<Req, Resp> {
    tx: Sender<Request<Req, Resp>>,
}

impl<Req, Resp> RequestSender<Req, Resp> {
    /// Send a request. Returns `Err(DispatcherError::ReceiverDisconnected)`
    /// when the backend worker has shut down.
    pub fn send(&self, request: Request<Req, Resp>) -> Result<(), DispatcherError> {
        self.tx
            .send(request)
            .map_err(|_| DispatcherError::ReceiverDisconnected)
    }
}

/// Consumer half of the request/reply channel — owned by the backend worker.
#[derive(Debug)]
pub struct RequestReceiver<Req, Resp> {
    rx: Receiver<Request<Req, Resp>>,
}

impl<Req, Resp> RequestReceiver<Req, Resp> {
    /// Block until a request arrives. Returns
    /// `Err(DispatcherError::SenderDisconnected)` when every sender has
    /// been dropped.
    pub fn recv(&self) -> Result<Request<Req, Resp>, DispatcherError> {
        self.rx
            .recv()
            .map_err(|_| DispatcherError::SenderDisconnected)
    }

    /// Non-blocking variant. `Ok(None)` when the channel is empty;
    /// `Err(SenderDisconnected)` when every sender has been dropped.
    pub fn try_recv(&self) -> Result<Option<Request<Req, Resp>>, DispatcherError> {
        match self.rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(None),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Err(DispatcherError::SenderDisconnected)
            }
        }
    }
}

/// Construct a request/reply channel pair with capacity `cap` on the
/// request queue. `cap == 0` is rejected (returns `Err`) because crossbeam
/// rendezvous channels would deadlock the request side; Phase 1 callers
/// always pick `cap >= 1`.
///
/// Caller pattern (UI thread, Phase 4 IconPicker example):
/// ```ignore
/// let (resp_tx, resp_rx) = crossbeam_channel::bounded(1);
/// request_sender.send(Request { req: IconRequest { path }, reply: resp_tx })?;
/// let icon_hash = resp_rx.recv()?; // blocks until backend replies
/// ```
///
/// Backend pattern (worker thread, T-100 future):
/// ```ignore
/// while let Ok(req) = receiver.recv() {
///     let resp = handle_load_icon(&req.req);
///     let _ = req.reply.send(resp); // ignore disconnect
/// }
/// ```
/// Pair returned by [`request_channel`] — kept as a named alias so the
/// signature stays under clippy's `type_complexity` threshold.
pub type RequestPair<Req, Resp> = (RequestSender<Req, Resp>, RequestReceiver<Req, Resp>);

pub fn request_channel<Req, Resp>(cap: usize) -> Result<RequestPair<Req, Resp>, DispatcherError> {
    if cap == 0 {
        // Rendezvous would force the request side to block until the
        // backend `recv()`s — Phase 1 expects fire-and-block-on-reply
        // semantics, not back-pressure on the send side. Fall back to a
        // 1-slot bounded channel internally is a possibility, but spec
        // §11 says surface the misuse explicitly so callers fix it.
        return Err(DispatcherError::ReceiverDisconnected);
    }
    let (tx, rx) = bounded(cap);
    Ok((RequestSender { tx }, RequestReceiver { rx }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_backend::rules::{Action, Condition, ConditionGroup, ConditionNode, RunMode};

    fn sample_rule(id: &str) -> Rule {
        Rule {
            id: SmolStr::new(id),
            name: "Archive desktop logs".to_string(),
            enabled: true,
            conditions: ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(
                vec![SmolStr::new_static("log")],
            ))]),
            actions: vec![Action::MoveToZone(SmolStr::new_static("archive"))],
            run_mode: RunMode::OnDemand,
            last_run: None,
            run_count: 0,
        }
    }

    #[test]
    fn drain_into_collects_all_pending() {
        let d = EventDispatcher::new();
        assert!(d.push(Command::TogglePin));
        assert!(d.push(Command::CreateZone(ZoneSpec {
            name: SmolStr::new_static("test"),
            origin: Point::ZERO,
            size: Size::new(200, 120),
        })));
        let mut buf: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
        let n = d.drain_into(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0], Command::TogglePin);
    }

    #[test]
    fn variant_name_matches_for_every_variant_shape() {
        // Spot-check across each variant family — guards against the
        // match drifting away from the variant identifier on rename.
        assert_eq!(Command::TogglePin.variant_name(), "TogglePin");
        assert_eq!(
            Command::ShowWindow(WindowKind::Main).variant_name(),
            "ShowWindow"
        );
        assert_eq!(Command::DeleteZone(ZoneId(1)).variant_name(), "DeleteZone");
        assert_eq!(Command::DuplicateZone.variant_name(), "DuplicateZone");
        assert_eq!(
            Command::ToggleSelectedZoneLock.variant_name(),
            "ToggleSelectedZoneLock"
        );
        assert_eq!(
            Command::ToggleAllZonesVisible.variant_name(),
            "ToggleAllZonesVisible"
        );
        assert_eq!(
            Command::ReflowVisibleZones.variant_name(),
            "ReflowVisibleZones"
        );
        assert_eq!(Command::FocusNextZone.variant_name(), "FocusNextZone");
        assert_eq!(
            Command::FocusPreviousZone.variant_name(),
            "FocusPreviousZone"
        );
        assert_eq!(
            Command::SetZoneIcon(ZoneId(1), SmolStr::new_static("folder")).variant_name(),
            "SetZoneIcon"
        );
        assert_eq!(
            Command::SetZoneAccent(ZoneId(1), Some(SmolStr::new_static("#3b82f6"))).variant_name(),
            "SetZoneAccent"
        );
        assert_eq!(
            Command::SetThemeBase(Some(SmolStr::new_static("#3b82f6"))).variant_name(),
            "SetThemeBase"
        );
        assert_eq!(
            Command::SetActiveTheme(SmolStr::new_static("ocean-blue")).variant_name(),
            "SetActiveTheme"
        );
        assert_eq!(
            Command::ImportTheme(SmolStr::new_static("C:/themes/custom.json")).variant_name(),
            "ImportTheme"
        );
        assert_eq!(Command::ListPlugins.variant_name(), "ListPlugins");
        assert_eq!(
            Command::InstallPlugin(SmolStr::new_static("C:/plugins/theme.bdplugin")).variant_name(),
            "InstallPlugin"
        );
        assert_eq!(
            Command::TogglePlugin(SmolStr::new_static("com.test.theme"), false).variant_name(),
            "TogglePlugin"
        );
        assert_eq!(
            Command::UninstallPlugin(SmolStr::new_static("com.test.theme")).variant_name(),
            "UninstallPlugin"
        );
        assert_eq!(
            Command::CopyItemPath(ItemPath::new("/path/file.txt")).variant_name(),
            "CopyItemPath"
        );
        assert_eq!(
            Command::OpenItemFile(ZoneId(1), ItemId(2)).variant_name(),
            "OpenItemFile"
        );
        assert_eq!(
            Command::MoveItem(ZoneId(1), ItemId(2), Point::ZERO).variant_name(),
            "MoveItem"
        );
        assert_eq!(
            Command::ToggleItemWide(ZoneId(1), ItemId(2)).variant_name(),
            "ToggleItemWide"
        );
        assert_eq!(
            Command::MoveItemToZone(ZoneId(1), ZoneId(3), ItemId(2)).variant_name(),
            "MoveItemToZone"
        );
        assert_eq!(
            Command::OpenItemFileRename(ZoneId(1), ItemId(2)).variant_name(),
            "OpenItemFileRename"
        );
        assert_eq!(
            Command::RenameItemFile(ZoneId(1), ItemId(2), SmolStr::new_static("report.txt"))
                .variant_name(),
            "RenameItemFile"
        );
        assert_eq!(
            Command::DeleteItemFileToRecycleBin(ZoneId(1), ItemId(2)).variant_name(),
            "DeleteItemFileToRecycleBin"
        );
        assert_eq!(
            Command::SetSetting {
                key: SmolStr::new_static("k"),
                value: SettingValue::Bool(true),
            }
            .variant_name(),
            "SetSetting"
        );
        assert_eq!(Command::OpenAbout.variant_name(), "OpenAbout");
        assert_eq!(
            Command::ToggleDebugOverlay.variant_name(),
            "ToggleDebugOverlay"
        );
        assert_eq!(Command::AutoOrganize.variant_name(), "AutoOrganize");
        assert_eq!(Command::OpenSearch.variant_name(), "OpenSearch");
        assert_eq!(
            Command::QuerySearch(SmolStr::new_static("contract")).variant_name(),
            "QuerySearch"
        );
        assert_eq!(
            Command::ActivateSearchResult(SmolStr::new_static("zone:1")).variant_name(),
            "ActivateSearchResult"
        );
        assert_eq!(Command::CloseSearch.variant_name(), "CloseSearch");
        assert_eq!(
            Command::ShowContextMenu {
                anchor: WindowHandle::NULL,
                items: Box::new(smallvec::SmallVec::new()),
            }
            .variant_name(),
            "ShowContextMenu"
        );
        assert_eq!(
            Command::CreateSettingsBackup.variant_name(),
            "CreateSettingsBackup"
        );
        assert_eq!(
            Command::ListSettingsBackups.variant_name(),
            "ListSettingsBackups"
        );
        assert_eq!(
            Command::RestoreLatestSettingsBackup.variant_name(),
            "RestoreLatestSettingsBackup"
        );
        assert_eq!(
            Command::RestoreSettingsBackup(SmolStr::new_static("200-new")).variant_name(),
            "RestoreSettingsBackup"
        );
        assert_eq!(
            Command::CreateRecoveryBundle.variant_name(),
            "CreateRecoveryBundle"
        );
        assert_eq!(
            Command::ExportRecoveryDiagnostics.variant_name(),
            "ExportRecoveryDiagnostics"
        );
        assert_eq!(
            Command::RestoreRecoveryBundle.variant_name(),
            "RestoreRecoveryBundle"
        );
        assert_eq!(
            Command::SetEncryptionPassphrase(SmolStr::new_static("secret")).variant_name(),
            "SetEncryptionPassphrase"
        );
        assert_eq!(
            Command::UnlockEncryptionPassphrase(SmolStr::new_static("secret")).variant_name(),
            "UnlockEncryptionPassphrase"
        );
        assert_eq!(
            Command::OpenLiveFolderPicker(ZoneId(1)).variant_name(),
            "OpenLiveFolderPicker"
        );
        assert_eq!(Command::CheckForUpdates.variant_name(), "CheckForUpdates");
        assert_eq!(Command::DownloadUpdate.variant_name(), "DownloadUpdate");
        assert_eq!(
            Command::InstallUpdateAndRestart.variant_name(),
            "InstallUpdateAndRestart"
        );
        assert_eq!(
            Command::SkipUpdateVersion(SmolStr::new_static("2.1.0")).variant_name(),
            "SkipUpdateVersion"
        );
        assert_eq!(
            Command::CaptureCapsule(SmolStr::new_static("Focus")).variant_name(),
            "CaptureCapsule"
        );
        assert_eq!(
            Command::RestoreCapsule(SmolStr::new_static("cap-1")).variant_name(),
            "RestoreCapsule"
        );
        assert_eq!(
            Command::DeleteCapsule(SmolStr::new_static("cap-1")).variant_name(),
            "DeleteCapsule"
        );
        assert_eq!(Command::OpenTimeline.variant_name(), "OpenTimeline");
        assert_eq!(
            Command::SaveCheckpoint {
                id: Some(SmolStr::new_static("cp-1")),
                label: None,
            }
            .variant_name(),
            "SaveCheckpoint"
        );
        assert_eq!(
            Command::RestoreCheckpoint(SmolStr::new_static("cp-1")).variant_name(),
            "RestoreCheckpoint"
        );
        assert_eq!(Command::UndoCheckpoint.variant_name(), "UndoCheckpoint");
        assert_eq!(Command::RedoCheckpoint.variant_name(), "RedoCheckpoint");
        assert_eq!(
            Command::DeleteCheckpoint(SmolStr::new_static("cp-1")).variant_name(),
            "DeleteCheckpoint"
        );
        assert_eq!(
            Command::OpenSnapshotPicker.variant_name(),
            "OpenSnapshotPicker"
        );
        assert_eq!(
            Command::SaveSnapshot {
                name: Some(SmolStr::new_static("manual")),
            }
            .variant_name(),
            "SaveSnapshot"
        );
        assert_eq!(
            Command::LoadSnapshot(SmolStr::new_static("snap-1")).variant_name(),
            "LoadSnapshot"
        );
        assert_eq!(
            Command::DeleteSnapshot(SmolStr::new_static("snap-1")).variant_name(),
            "DeleteSnapshot"
        );
        assert_eq!(
            Command::SaveRule(Box::new(sample_rule("rule-1"))).variant_name(),
            "SaveRule"
        );
        assert_eq!(
            Command::DeleteRule(SmolStr::new_static("rule-1")).variant_name(),
            "DeleteRule"
        );
        assert_eq!(
            Command::PreviewRuleHits(Box::new(sample_rule("rule-1"))).variant_name(),
            "PreviewRuleHits"
        );
        assert_eq!(
            Command::RunRuleNow(SmolStr::new_static("rule-1")).variant_name(),
            "RunRuleNow"
        );
        assert_eq!(
            Command::BulkDeleteZones(vec![ZoneId(7)]).variant_name(),
            "BulkDeleteZones"
        );
        assert_eq!(
            Command::BulkSetZonesVisible {
                ids: vec![ZoneId(7)],
                visible: false,
            }
            .variant_name(),
            "BulkSetZonesVisible"
        );
        assert_eq!(
            Command::BulkApplyLayout {
                ids: vec![ZoneId(7)],
                algorithm: BulkLayoutAlgorithm::Grid,
            }
            .variant_name(),
            "BulkApplyLayout"
        );
        assert_eq!(
            Command::BulkUpdateZones(vec![BulkZoneUpdate {
                id: ZoneId(7),
                locked: Some(true),
                ..BulkZoneUpdate::default()
            }])
            .variant_name(),
            "BulkUpdateZones"
        );
        assert_eq!(
            Command::BulkMoveZones {
                ids: vec![ZoneId(7)],
                delta: Point::new(4, 5),
            }
            .variant_name(),
            "BulkMoveZones"
        );
        assert_eq!(Command::QuitApp.variant_name(), "QuitApp");
    }

    #[test]
    fn unhandled_command_log_does_not_panic_on_any_variant() {
        // Construct one of every variant family and feed it to the
        // unhandled-log helper. Release builds compile this to a no-op,
        // debug builds emit one OutputDebugStringA per call. Either way,
        // this test guards spec §11 (no panic on the dispatcher path).
        let cases = [
            Command::TogglePin,
            Command::ToggleSettings,
            Command::CloseSettings,
            Command::ToggleLocale,
            Command::ShowTrayMenu,
            Command::ShowWindow(WindowKind::Main),
            Command::HideWindow(WindowKind::IconPicker),
            Command::CreateZone(ZoneSpec {
                name: SmolStr::new_static("z"),
                origin: Point::ZERO,
                size: Size::new(200, 120),
            }),
            Command::DeleteZone(ZoneId(7)),
            Command::RenameZone(ZoneId(7), SmolStr::new_static("new")),
            Command::MoveZone(ZoneId(7), Point::new(10, 20)),
            Command::ResizeZone(ZoneId(7), Size::new(300, 200)),
            Command::StackZone(ZoneId(7), ZoneId(8)),
            Command::UnstackZone(ZoneId(7)),
            Command::OpenStackTray(ZoneId(7)),
            Command::CloseStackTray,
            Command::PreviewStackMember(ZoneId(7), ZoneId(8)),
            Command::DetachStackMember(ZoneId(7), ZoneId(8)),
            Command::DissolveStack(ZoneId(7)),
            Command::ReorderStackMember(ZoneId(7), ZoneId(8), 1),
            Command::SetZoneAlias(ZoneId(7), SmolStr::new_static("alias")),
            Command::SetZoneIcon(ZoneId(7), SmolStr::new_static("folder")),
            Command::SetZoneAccent(ZoneId(7), Some(SmolStr::new_static("#3b82f6"))),
            Command::SetZoneAccent(ZoneId(7), None),
            Command::SetThemeBase(Some(SmolStr::new_static("#3b82f6"))),
            Command::SetThemeBase(None),
            Command::SetActiveTheme(SmolStr::new_static("ocean-blue")),
            Command::ImportTheme(SmolStr::new_static("C:/themes/custom.json")),
            Command::ListPlugins,
            Command::InstallPlugin(SmolStr::new_static("C:/plugins/theme.bdplugin")),
            Command::TogglePlugin(SmolStr::new_static("com.test.theme"), true),
            Command::UninstallPlugin(SmolStr::new_static("com.test.theme")),
            Command::SetZoneGridColumns(ZoneId(7), 5),
            Command::SetZoneCapsule(
                ZoneId(7),
                SmolStr::new_static("large"),
                SmolStr::new_static("rounded"),
            ),
            Command::OpenLiveFolderPicker(ZoneId(7)),
            Command::BindZoneToFolder(ZoneId(7), SmolStr::new_static("C:/Users/HP/Documents")),
            Command::UnbindZoneFolder(ZoneId(7)),
            Command::RefreshLiveFolder(ZoneId(7)),
            Command::ReorderZone(ZoneId(7), 3),
            Command::DuplicateZone,
            Command::ToggleSelectedZoneLock,
            Command::ToggleAllZonesVisible,
            Command::ReflowVisibleZones,
            Command::FocusNextZone,
            Command::FocusPreviousZone,
            Command::AddItem(ZoneId(7), ItemPath::new("/path/file.txt")),
            Command::RemoveItem(ZoneId(7), ItemId(99)),
            Command::OpenItemFile(ZoneId(7), ItemId(99)),
            Command::CopyItemPath(ItemPath::new("/path/file.txt")),
            Command::MoveItem(ZoneId(7), ItemId(99), Point::ZERO),
            Command::ToggleItemWide(ZoneId(7), ItemId(99)),
            Command::MoveItemToZone(ZoneId(7), ZoneId(8), ItemId(99)),
            Command::OpenItemFileRename(ZoneId(7), ItemId(99)),
            Command::RenameItemFile(ZoneId(7), ItemId(99), SmolStr::new_static("renamed.txt")),
            Command::DeleteItemFileToRecycleBin(ZoneId(7), ItemId(99)),
            Command::OpenSettings,
            Command::OpenAbout,
            Command::CloseAbout,
            Command::ToggleDebugOverlay,
            Command::SetSetting {
                key: SmolStr::new_static("show_in_taskbar"),
                value: SettingValue::Bool(true),
            },
            Command::ResetKeybinding {
                action: SmolStr::new_static("timeline.open"),
            },
            Command::CreateSettingsBackup,
            Command::ListSettingsBackups,
            Command::RestoreLatestSettingsBackup,
            Command::RestoreSettingsBackup(SmolStr::new_static("200-new")),
            Command::CreateRecoveryBundle,
            Command::ExportRecoveryDiagnostics,
            Command::RestoreRecoveryBundle,
            Command::SetEncryptionPassphrase(SmolStr::new_static("secret")),
            Command::UnlockEncryptionPassphrase(SmolStr::new_static("secret")),
            Command::CheckForUpdates,
            Command::DownloadUpdate,
            Command::InstallUpdateAndRestart,
            Command::SkipUpdateVersion(SmolStr::new_static("2.1.0")),
            Command::AutoOrganize,
            Command::OpenSearch,
            Command::QuerySearch(SmolStr::new_static("contract")),
            Command::ActivateSearchResult(SmolStr::new_static("zone:7")),
            Command::CloseSearch,
            Command::LoadIcon(ItemPath::new("/path/file.txt")),
            Command::OpenIconPicker {
                zone_id: Some(ZoneId(7)),
            },
            Command::OpenIconPicker { zone_id: None },
            Command::OpenPalettePicker {
                target: PaletteTarget::ZoneAccent(ZoneId(7)),
            },
            Command::OpenPalettePicker {
                target: PaletteTarget::ThemeBase,
            },
            Command::OpenCapsulePicker,
            Command::CaptureCapsule(SmolStr::new_static("Focus")),
            Command::RestoreCapsule(SmolStr::new_static("cap-1")),
            Command::DeleteCapsule(SmolStr::new_static("cap-1")),
            Command::OpenTimeline,
            Command::SaveCheckpoint {
                id: None,
                label: Some(SmolStr::new_static("manual")),
            },
            Command::RestoreCheckpoint(SmolStr::new_static("cp-1")),
            Command::UndoCheckpoint,
            Command::RedoCheckpoint,
            Command::DeleteCheckpoint(SmolStr::new_static("cp-1")),
            Command::OpenSnapshotPicker,
            Command::SaveSnapshot {
                name: Some(SmolStr::new_static("manual")),
            },
            Command::LoadSnapshot(SmolStr::new_static("snap-1")),
            Command::DeleteSnapshot(SmolStr::new_static("snap-1")),
            Command::OpenRulesWizard,
            Command::SaveRule(Box::new(sample_rule(""))),
            Command::DeleteRule(SmolStr::new_static("rule-1")),
            Command::PreviewRuleHits(Box::new(sample_rule("rule-1"))),
            Command::RunRuleNow(SmolStr::new_static("rule-1")),
            Command::OpenBulkManager,
            Command::BulkDeleteZones(vec![ZoneId(7), ZoneId(8)]),
            Command::BulkSetZonesVisible {
                ids: vec![ZoneId(7), ZoneId(8)],
                visible: false,
            },
            Command::BulkApplyLayout {
                ids: vec![ZoneId(7), ZoneId(8)],
                algorithm: BulkLayoutAlgorithm::Organic,
            },
            Command::BulkUpdateZones(vec![BulkZoneUpdate {
                id: ZoneId(7),
                position: Some(Point::new(10, 20)),
                size: Some(Size::new(240, 160)),
                accent_color: Some(SmolStr::new_static("#3b82f6")),
                capsule_size: Some(SmolStr::new_static("large")),
                locked: Some(true),
                alias: Some(SmolStr::new_static("Focus")),
                display_mode: Some(Some(SmolStr::new_static("hover"))),
                icon: Some(SmolStr::new_static("folder")),
            }]),
            Command::BulkMoveZones {
                ids: vec![ZoneId(7), ZoneId(8)],
                delta: Point::new(20, -10),
            },
            Command::OpenZoneEditor(ZoneId(7)),
            Command::ShowSuggestor,
            Command::PinZoneAsMinibar(ZoneId(7)),
            Command::UnpinMinibar(ZoneId(7)),
            Command::ListPinnedMinibars,
            Command::ShowTooltip {
                anchor: WindowHandle::NULL,
                text: SmolStr::new_static("hi"),
            },
            Command::HideTooltip,
            Command::ShowContextMenu {
                anchor: WindowHandle::NULL,
                items: Box::new(smallvec::SmallVec::new()),
            },
            Command::HideContextMenu,
            Command::QuitApp,
        ];
        for cmd in &cases {
            unhandled_command_log(cmd);
        }
    }

    // -------- T-014 request/reply channel round-trip --------

    #[test]
    fn request_channel_rejects_zero_capacity() {
        let result = request_channel::<IconRequest, IconHash>(0);
        assert!(result.is_err());
    }

    #[test]
    fn request_reply_round_trip_blocks_until_backend_replies() {
        // Construct the request channel between "UI thread" (this test) and
        // a synthetic backend worker on a std::thread. NO tokio anywhere.
        let (req_tx, req_rx) = request_channel::<IconRequest, IconHash>(8).expect("nonzero cap");

        // Backend worker: drain one request, compute a deterministic
        // response, send it back through the per-request reply channel.
        let backend = std::thread::spawn(move || {
            let req = req_rx.recv().expect("at least one request");
            // Synthesize a response — first byte of the path's bytes goes
            // into the hash so the test asserts the path round-tripped
            // correctly through the request payload.
            let mut bytes = [0u8; 16];
            if let Some(b) = req.req.path.0.as_bytes().first() {
                bytes[0] = *b;
            }
            let _ = req.reply.send(IconHash(bytes));
        });

        // UI side: build a one-shot reply channel, send the request, block
        // on the reply.
        let (resp_tx, resp_rx) = crossbeam_channel::bounded(1);
        req_tx
            .send(Request {
                req: IconRequest {
                    path: ItemPath::new("/desktop/file.png"),
                },
                reply: resp_tx,
            })
            .expect("backend alive");
        let hash = resp_rx.recv().expect("backend replied");
        assert_eq!(hash.0[0], b'/'); // first byte of "/desktop/file.png"

        backend.join().expect("backend joined cleanly");
    }

    #[test]
    fn request_sender_reports_disconnected_when_backend_drops_receiver() {
        let (req_tx, req_rx) = request_channel::<IconRequest, IconHash>(1).expect("nonzero cap");
        drop(req_rx); // simulate backend shutdown

        let (resp_tx, _resp_rx) = crossbeam_channel::bounded(1);
        let err = req_tx
            .send(Request {
                req: IconRequest {
                    path: ItemPath::new("anything"),
                },
                reply: resp_tx,
            })
            .expect_err("send must fail after receiver drop");
        assert_eq!(err, DispatcherError::ReceiverDisconnected);
    }

    #[test]
    fn request_receiver_try_recv_returns_none_on_empty_channel() {
        let (_req_tx, req_rx) = request_channel::<IconRequest, IconHash>(1).expect("nonzero cap");
        let result = req_rx.try_recv().expect("not disconnected");
        assert!(result.is_none(), "empty channel must yield Ok(None)");
    }

    #[test]
    fn request_receiver_try_recv_reports_disconnected_after_sender_drops() {
        let (req_tx, req_rx) = request_channel::<IconRequest, IconHash>(1).expect("nonzero cap");
        drop(req_tx); // simulate every UI sender released
        let err = req_rx
            .try_recv()
            .expect_err("must surface SenderDisconnected once last sender drops");
        assert_eq!(err, DispatcherError::SenderDisconnected);
    }

    // -------- ΔB ruling — serde round-trip on the closed enum --------

    #[test]
    fn command_serde_round_trip_for_payload_variants() {
        // Spec ΔB: serde derive is forward-compat surface, never used at
        // runtime in Phase 1. Round-trip a representative sample so a
        // future PR cannot quietly remove the derive without this failing.
        let original = Command::CreateZone(ZoneSpec {
            name: SmolStr::new_static("Zone-α"),
            origin: Point::new(10, 20),
            size: Size::new(300, 200),
        });
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: Command = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }
}
