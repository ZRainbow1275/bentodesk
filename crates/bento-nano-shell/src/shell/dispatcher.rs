//! Exhaustive command classification and UI-thread dispatcher pump.

use super::*;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct DispatchEffects {
    pub(super) needs_redraw: bool,
    pub(super) quit_after_drain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchGroup {
    Window,
    Zones,
    Stacks,
    ItemsSettings,
    RecoveryUpdates,
    Workflows,
    BulkSurfaces,
}

impl DispatchGroup {
    fn for_command(command: &Command) -> Self {
        match command {
            Command::TogglePin => Self::Window,
            Command::ToggleSettings => Self::Window,
            Command::CloseSettings => Self::Window,
            Command::OpenSettings => Self::Window,
            Command::OpenAbout => Self::Window,
            Command::CloseAbout => Self::Window,
            Command::ToggleDebugOverlay => Self::Window,
            Command::ToggleLocale => Self::Window,
            Command::HideWindow(..) => Self::Window,
            Command::ShowWindow(..) => Self::Window,
            Command::ShowTrayMenu => Self::Window,
            Command::CreateZone(..) => Self::Zones,
            Command::DeleteZone(..) => Self::Zones,
            Command::RenameZone(..) => Self::Zones,
            Command::MoveZone(..) => Self::Zones,
            Command::ResizeZone(..) => Self::Zones,
            Command::SetZoneAlias(..) => Self::Zones,
            Command::SetZoneIcon(..) => Self::Zones,
            Command::SetZoneAccent(..) => Self::Zones,
            Command::SetThemeBase(..) => Self::Zones,
            Command::SetActiveTheme(..) => Self::Zones,
            Command::ImportTheme(..) => Self::Zones,
            Command::ListPlugins => Self::Zones,
            Command::InstallPlugin(..) => Self::Zones,
            Command::TogglePlugin(..) => Self::Zones,
            Command::UninstallPlugin(..) => Self::Zones,
            Command::SetZoneGridColumns(..) => Self::Stacks,
            Command::SetZoneCapsule(..) => Self::Stacks,
            Command::OpenLiveFolderPicker(..) => Self::Stacks,
            Command::BindZoneToFolder(..) => Self::Stacks,
            Command::UnbindZoneFolder(..) => Self::Stacks,
            Command::RefreshLiveFolder(..) => Self::Stacks,
            Command::ReorderZone(..) => Self::Stacks,
            Command::AutoArrangeZone(..) => Self::Stacks,
            Command::DuplicateZone => Self::Stacks,
            Command::ToggleSelectedZoneLock => Self::Stacks,
            Command::ToggleAllZonesVisible => Self::Stacks,
            Command::ReflowVisibleZones => Self::Stacks,
            Command::FocusNextZone => Self::Stacks,
            Command::FocusPreviousZone => Self::Stacks,
            Command::StackZone(..) => Self::Stacks,
            Command::UnstackZone(..) => Self::Stacks,
            Command::OpenStackTray(..) => Self::Stacks,
            Command::CloseStackTray => Self::Stacks,
            Command::PreviewStackMember(..) => Self::Stacks,
            Command::ToggleStackBloomPreview(..) => Self::Stacks,
            Command::DetachStackMember(..) => Self::Stacks,
            Command::DissolveStack(..) => Self::Stacks,
            Command::ReorderStackMember(..) => Self::Stacks,
            Command::AddItem(..) => Self::ItemsSettings,
            Command::RemoveItem(..) => Self::ItemsSettings,
            Command::OpenItemFile(..) => Self::ItemsSettings,
            Command::OpenItemFileRename(..) => Self::ItemsSettings,
            Command::RenameItemFile(..) => Self::ItemsSettings,
            Command::DeleteItemFileToRecycleBin(..) => Self::ItemsSettings,
            Command::CopyItemPath(..) => Self::ItemsSettings,
            Command::MoveItem(..) => Self::ItemsSettings,
            Command::ToggleItemWide(..) => Self::ItemsSettings,
            Command::MoveItemToZone(..) => Self::ItemsSettings,
            Command::SetSetting { .. } => Self::ItemsSettings,
            Command::ResetKeybinding { .. } => Self::ItemsSettings,
            Command::CreateSettingsBackup => Self::RecoveryUpdates,
            Command::ListSettingsBackups => Self::RecoveryUpdates,
            Command::RestoreLatestSettingsBackup => Self::RecoveryUpdates,
            Command::RestoreSettingsBackup(..) => Self::RecoveryUpdates,
            Command::CreateRecoveryBundle => Self::RecoveryUpdates,
            Command::ExportRecoveryDiagnostics => Self::RecoveryUpdates,
            Command::RestoreRecoveryBundle => Self::RecoveryUpdates,
            Command::SetEncryptionPassphrase(..) => Self::RecoveryUpdates,
            Command::UnlockEncryptionPassphrase(..) => Self::RecoveryUpdates,
            Command::CheckForUpdates => Self::RecoveryUpdates,
            Command::DownloadUpdate => Self::RecoveryUpdates,
            Command::InstallUpdateAndRestart => Self::RecoveryUpdates,
            Command::SkipUpdateVersion(..) => Self::RecoveryUpdates,
            Command::AutoOrganize => Self::Workflows,
            Command::LoadIcon(..) => Self::Workflows,
            Command::ApplyLoadedIcon { .. } => Self::Workflows,
            Command::OpenIconPicker { .. } => Self::Workflows,
            Command::OpenPalettePicker { .. } => Self::Workflows,
            Command::OpenCapsulePicker => Self::Workflows,
            Command::CaptureCapsule(..) => Self::Workflows,
            Command::RestoreCapsule(..) => Self::Workflows,
            Command::DeleteCapsule(..) => Self::Workflows,
            Command::OpenTimeline => Self::Workflows,
            Command::OpenSnapshotPicker => Self::Workflows,
            Command::SaveSnapshot { .. } => Self::Workflows,
            Command::LoadSnapshot(..) => Self::Workflows,
            Command::DeleteSnapshot(..) => Self::Workflows,
            Command::SaveCheckpoint { .. } => Self::Workflows,
            Command::RestoreCheckpoint(..) => Self::Workflows,
            Command::UndoCheckpoint => Self::Workflows,
            Command::RedoCheckpoint => Self::Workflows,
            Command::DeleteCheckpoint(..) => Self::Workflows,
            Command::OpenRulesWizard => Self::Workflows,
            Command::SaveRule(..) => Self::Workflows,
            Command::DeleteRule(..) => Self::Workflows,
            Command::PreviewRuleHits(..) => Self::Workflows,
            Command::RunRuleNow(..) => Self::Workflows,
            Command::OpenBulkManager => Self::BulkSurfaces,
            Command::BulkDeleteZones(..) => Self::BulkSurfaces,
            Command::BulkSetZonesVisible { .. } => Self::BulkSurfaces,
            Command::BulkApplyLayout { .. } => Self::BulkSurfaces,
            Command::BulkUpdateZones(..) => Self::BulkSurfaces,
            Command::BulkMoveZones { .. } => Self::BulkSurfaces,
            Command::OpenZoneEditor(..) => Self::BulkSurfaces,
            Command::ShowSuggestor => Self::BulkSurfaces,
            Command::OpenSearch => Self::BulkSurfaces,
            Command::QuerySearch(..) => Self::BulkSurfaces,
            Command::ActivateSearchResult(..) => Self::BulkSurfaces,
            Command::CloseSearch => Self::BulkSurfaces,
            Command::PinZoneAsMinibar(..) => Self::BulkSurfaces,
            Command::UnpinMinibar(..) => Self::BulkSurfaces,
            Command::ListPinnedMinibars => Self::BulkSurfaces,
            Command::ShowTooltip { .. } => Self::BulkSurfaces,
            Command::HideTooltip => Self::BulkSurfaces,
            Command::GroupingApply { .. } => Self::BulkSurfaces,
            Command::SuggestorDismiss { .. } => Self::BulkSurfaces,
            Command::ShowContextMenu { .. } => Self::BulkSurfaces,
            Command::HideContextMenu => Self::BulkSurfaces,
            Command::QuitApp => Self::BulkSurfaces,
        }
    }
}

pub(super) fn consume_dispatcher(root: &AppRoot, hwnd: HWND) {
    let mut buf: smallvec::SmallVec<[Command; 8]> = smallvec::SmallVec::new();
    let mut effects = DispatchEffects::default();
    // A command may synchronously enqueue a follow-up command. Drain until
    // the UI-thread queue is stable so one input completes the full action.
    while root.dispatcher.drain_into(&mut buf) > 0 {
        for command in buf.drain(..) {
            match DispatchGroup::for_command(&command) {
                DispatchGroup::Window => {
                    dispatch_window::dispatch(root, hwnd, command, &mut effects)
                }
                DispatchGroup::Zones => dispatch_zones::dispatch(root, hwnd, command, &mut effects),
                DispatchGroup::Stacks => {
                    dispatch_stacks::dispatch(root, hwnd, command, &mut effects)
                }
                DispatchGroup::ItemsSettings => {
                    dispatch_items_settings::dispatch(root, hwnd, command, &mut effects)
                }
                DispatchGroup::RecoveryUpdates => {
                    dispatch_recovery_updates::dispatch(root, hwnd, command, &mut effects)
                }
                DispatchGroup::Workflows => {
                    dispatch_workflows::dispatch(root, hwnd, command, &mut effects)
                }
                DispatchGroup::BulkSurfaces => {
                    dispatch_bulk_surfaces::dispatch(root, hwnd, command, &mut effects)
                }
            }
        }
    }

    flush_dirty_zones(root);
    if effects.needs_redraw {
        request_redraw(hwnd);
    }
    if effects.quit_after_drain {
        let target = find_main_hwnd(root).unwrap_or(hwnd);
        if target.is_null() {
            // SAFETY: PostQuitMessage is canonical when no owned HWND is left.
            unsafe { PostQuitMessage(0) };
        } else {
            // SAFETY: WM_DESTROY centralizes final persistence and teardown.
            unsafe { DestroyWindow(target) };
        }
    }
}
