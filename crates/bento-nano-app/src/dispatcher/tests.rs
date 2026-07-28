use super::*;
use bento_nano_backend::rules::{Action, Condition, ConditionGroup, ConditionNode, RunMode};

fn sample_rule(id: &str) -> Rule {
    Rule {
        id: SmolStr::new(id),
        name: "Archive desktop logs".to_string(),
        enabled: true,
        conditions: ConditionGroup::All(vec![ConditionNode::Leaf(Condition::ExtensionIn(vec![
            SmolStr::new_static("log"),
        ]))]),
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
        Command::ToggleStackBloomPreview(ZoneId(7), ZoneId(8)),
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
        Command::BindZoneToFolder(
            ZoneId(7),
            SmolStr::new_static("C:/Users/BentoDeskTest/Documents"),
        ),
        Command::UnbindZoneFolder(ZoneId(7)),
        Command::RefreshLiveFolder(ZoneId(7)),
        Command::ReorderZone(ZoneId(7), 3),
        Command::AutoArrangeZone(ZoneId(7)),
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
        Command::ApplyLoadedIcon {
            path: ItemPath::new("/path/file.txt"),
            hash: SmolStr::new_static("0123456789abcdef"),
        },
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
