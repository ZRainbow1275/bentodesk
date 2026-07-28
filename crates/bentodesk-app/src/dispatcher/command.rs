use super::*;

// -----------------------------------------------------------------------------
// Command — the closed enum every UI/background producer dispatches into.
// Phase 1 expansion: ~30 variants spanning Zone CRUD, Item CRUD, Settings,
// Icon ops, MiniBar, Tooltip, ContextMenu, Window lifecycle.
// -----------------------------------------------------------------------------

/// Application command — one of these is dispatched per UI event.
///
/// Closed enum (no `Box<dyn Any>`) so the consumer's match in
/// `bentodesk-shell::main::consume_dispatcher` (or the per-window wndproc
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
    /// Click a Bloom petal: commit its preview as sticky, or close it when the
    /// same sticky petal is clicked again. Kept separate from keyboard/tray
    /// selection because a hover-open preview must become sticky rather than
    /// being toggled closed by the first explicit click.
    ToggleStackBloomPreview(ZoneId, ZoneId),
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
    /// Sort only this zone's items by display name and rebuild its grid.
    AutoArrangeZone(ZoneId),
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
    /// `bentodesk_backend::icon::protocol::extract_and_cache` directly.
    LoadIcon(ItemPath),
    /// Apply an icon hash produced off the UI thread. Startup cache repair uses
    /// this result path so slow Shell/COM icon handlers never block painting.
    ApplyLoadedIcon { path: ItemPath, hash: SmolStr },

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
    /// the payload to `bentodesk_backend::grouping::apply_auto_group`
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
            Self::ToggleStackBloomPreview(_, _) => "ToggleStackBloomPreview",
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
            Self::AutoArrangeZone(_) => "AutoArrangeZone",
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
            Self::ApplyLoadedIcon { .. } => "ApplyLoadedIcon",
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
/// which DebugView monitors — same channel `bentodesk-platform::allocator`
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
