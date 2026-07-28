//! Command handlers for the `workflows` domain.

use super::*;

pub(super) fn dispatch(
    root: &AppRoot,
    _hwnd: HWND,
    command: Command,
    effects: &mut DispatchEffects,
) {
    match command {
        Command::AutoOrganize => {
            // Match the Tauri tray contract: explicit Auto Organize
            // opens the reviewable suggestor for a concrete target
            // Zone. It must not silently mint every suggestion at the
            // same origin.
            show_suggestor(root);
            effects.needs_redraw = true;
        }
        Command::LoadIcon(path) => {
            if let Some(hash) = load_icon_hash_for_path(&path) {
                if apply_loaded_item_icon(root, &path, hash.as_str()) {
                    effects.needs_redraw = true;
                }
            }
        }
        Command::ApplyLoadedIcon { path, hash } => {
            if apply_loaded_item_icon(root, &path, hash.as_str()) {
                effects.needs_redraw = true;
            }
        }
        Command::OpenIconPicker { zone_id } => {
            // F2-07 — open the IconPicker aux HWND. Mirrors the F2-05
            // tooltip / F2-06 popover minimal-reachability shape:
            // construct the business descriptor (so `business::icon_picker`
            // is reachable from the production binary), lazy-spawn the
            // aux HWND via the F2-02 factory, then show with foreground
            // activation. The selection-result follow-up Command
            // (`SetZoneIcon`) belongs to the F3 wave.
            open_icon_picker(root, zone_id);
        }
        Command::OpenPalettePicker { target } => {
            // F2-07 — open the palette picker aux HWND. f2-foundations
            // landed `WindowKind::PalettePicker` (Task #10) so the
            // dedicated 320× 240 chrome from `default_size` is honoured.
            // Follow-up Command (`SetZoneAccent` / `SetThemeBase`) is F3.
            open_palette_picker(root, target);
        }
        Command::OpenCapsulePicker => {
            // Open the Context Capsule browser aux HWND. The selected-stack
            // port now backs it with real zones.bin snapshots under the
            // appdata sibling `capsules/` directory.
            open_capsule_picker(root);
        }
        Command::CaptureCapsule(name) => {
            match capture_context_capsule(root, name.as_str()) {
                Ok(entry) => {
                    clear_context_capsule_picker_error(root);
                    log_static(
                        format!(
                            "capsule: CaptureCapsule id={} name={}\n",
                            entry.id, entry.name
                        )
                        .as_str(),
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::capsules",
                        error = %error,
                        "CaptureCapsule failed"
                    );
                    set_context_capsule_picker_error(
                        root,
                        localized_current(
                            format!("捕获失败：{error}"),
                            format!("Capture failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::CapsulePicker) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::RestoreCapsule(capsule_id) => {
            match restore_context_capsule(root, capsule_id.as_str()) {
                Ok(restored_count) => {
                    clear_context_capsule_picker_error(root);
                    log_static(
                        format!(
                            "capsule: RestoreCapsule id={} zones={}\n",
                            capsule_id, restored_count
                        )
                        .as_str(),
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::capsules",
                        error = %error,
                        "RestoreCapsule failed"
                    );
                    set_context_capsule_picker_error(
                        root,
                        localized_current(
                            format!("恢复失败：{error}"),
                            format!("Restore failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::CapsulePicker) {
                request_redraw(target);
            }
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::DeleteCapsule(capsule_id) => {
            match delete_context_capsule(root, capsule_id.as_str()) {
                Ok(()) => {
                    clear_context_capsule_picker_error(root);
                    log_static(format!("capsule: DeleteCapsule id={capsule_id}\n").as_str());
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::capsules",
                        error = %error,
                        "DeleteCapsule failed"
                    );
                    set_context_capsule_picker_error(
                        root,
                        localized_current(
                            format!("删除失败：{error}"),
                            format!("Delete failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::CapsulePicker) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::OpenTimeline => {
            open_timeline(root);
        }
        Command::OpenSnapshotPicker => {
            open_snapshot_picker(root);
        }
        Command::SaveSnapshot { name } => {
            match save_layout_snapshot(root, name) {
                Ok(snapshot) => tracing::info!(
                    target: "bentodesk::snapshot",
                    snapshot_id = %snapshot.id,
                    "SaveSnapshot persisted selected-stack layout snapshot"
                ),
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::snapshot",
                        error = %error,
                        "SaveSnapshot failed"
                    );
                    set_snapshot_picker_error(
                        root,
                        localized_current(
                            format!("保存失败：{error}"),
                            format!("Save failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::SnapshotPicker) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::LoadSnapshot(snapshot_id) => {
            match load_layout_snapshot(root, snapshot_id.as_str()) {
                Ok(snapshot) => tracing::info!(
                    target: "bentodesk::snapshot",
                    snapshot_id = %snapshot.id,
                    "LoadSnapshot applied selected-stack layout"
                ),
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::snapshot",
                        snapshot_id = %snapshot_id,
                        error = %error,
                        "LoadSnapshot failed"
                    );
                    set_snapshot_picker_error(
                        root,
                        localized_current(
                            format!("载入失败：{error}"),
                            format!("Load failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::SnapshotPicker) {
                request_redraw(target);
            }
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::DeleteSnapshot(snapshot_id) => {
            match delete_layout_snapshot(root, snapshot_id.as_str()) {
                Ok(()) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::snapshot",
                        snapshot_id = %snapshot_id,
                        error = %error,
                        "DeleteSnapshot failed"
                    );
                    set_snapshot_picker_error(
                        root,
                        localized_current(
                            format!("删除失败：{error}"),
                            format!("Delete failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::SnapshotPicker) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::SaveCheckpoint { id, label } => {
            match save_timeline_checkpoint(root, id, label) {
                Ok(checkpoint) => tracing::info!(
                    target: "bentodesk::timeline",
                    checkpoint_id = %checkpoint.id,
                    pinned = checkpoint.pinned,
                    "SaveCheckpoint persisted selected-stack checkpoint"
                ),
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::timeline",
                        error = %error,
                        "SaveCheckpoint failed"
                    );
                    set_timeline_error(
                        root,
                        localized_current(
                            format!("保存失败：{error}"),
                            format!("Save failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::Timeline) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::RestoreCheckpoint(checkpoint_id) => {
            match restore_timeline_checkpoint(root, checkpoint_id.as_str()) {
                Ok(restored_id) => tracing::info!(
                    target: "bentodesk::timeline",
                    checkpoint_id = %restored_id,
                    "RestoreCheckpoint applied selected-stack layout"
                ),
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::timeline",
                        checkpoint_id = %checkpoint_id,
                        error = %error,
                        "RestoreCheckpoint failed"
                    );
                    set_timeline_error(
                        root,
                        localized_current(
                            format!("恢复失败：{error}"),
                            format!("Restore failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::Timeline) {
                request_redraw(target);
            }
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::UndoCheckpoint => {
            match undo_timeline_checkpoint(root) {
                Ok(Some(checkpoint_id)) => tracing::info!(
                    target: "bentodesk::timeline",
                    checkpoint_id = %checkpoint_id,
                    "UndoCheckpoint applied selected-stack layout"
                ),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::timeline",
                        error = %error,
                        "UndoCheckpoint failed"
                    );
                    set_timeline_error(
                        root,
                        localized_current(
                            format!("撤销失败：{error}"),
                            format!("Undo failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::Timeline) {
                request_redraw(target);
            }
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::RedoCheckpoint => {
            match redo_timeline_checkpoint(root) {
                Ok(Some(checkpoint_id)) => tracing::info!(
                    target: "bentodesk::timeline",
                    checkpoint_id = %checkpoint_id,
                    "RedoCheckpoint applied selected-stack layout"
                ),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::timeline",
                        error = %error,
                        "RedoCheckpoint failed"
                    );
                    set_timeline_error(
                        root,
                        localized_current(
                            format!("重做失败：{error}"),
                            format!("Redo failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::Timeline) {
                request_redraw(target);
            }
            if let Some(target) = find_main_hwnd(root) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::DeleteCheckpoint(checkpoint_id) => {
            match delete_timeline_checkpoint(root, checkpoint_id.as_str()) {
                Ok(()) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::timeline",
                        checkpoint_id = %checkpoint_id,
                        error = %error,
                        "DeleteCheckpoint failed"
                    );
                    set_timeline_error(
                        root,
                        localized_current(
                            format!("删除失败：{error}"),
                            format!("Delete failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::Timeline) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::OpenRulesWizard => {
            // F2-08 — open the multi-step rules wizard aux HWND
            // (`business::rules_wizard`). Save/finish emits a follow-up
            // F3 `AddRule` / `UpdateRule` Command (deferred to F3).
            open_rules_wizard(root);
        }
        Command::SaveRule(rule) => {
            match persist_rule_for_wizard(root, *rule) {
                Ok(()) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::rules",
                        error = %error,
                        "SaveRule failed"
                    );
                    set_rules_wizard_error(
                        root,
                        localized_current(
                            format!("保存失败：{error}"),
                            format!("Save failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::RulesWizard) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::DeleteRule(rule_id) => {
            match delete_rule_for_wizard(root, rule_id.as_str()) {
                Ok(()) => {
                    clear_rules_wizard_delete_confirmation(root);
                }
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::rules",
                        rule_id = %rule_id,
                        error = %error,
                        "DeleteRule failed"
                    );
                    set_rules_wizard_error(
                        root,
                        localized_current(
                            format!("删除失败：{error}"),
                            format!("Delete failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::RulesWizard) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::PreviewRuleHits(rule) => {
            match preview_rule_for_wizard(root, &rule) {
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::rules",
                        error = %error,
                        "PreviewRuleHits failed"
                    );
                    set_rules_wizard_error(
                        root,
                        localized_current(
                            format!("预览失败：{error}"),
                            format!("Preview failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::RulesWizard) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        Command::RunRuleNow(rule_id) => {
            match run_rule_now_for_wizard(root, &rule_id) {
                Ok(report) => tracing::info!(
                    target: "bentodesk::rules",
                    rule_id = %rule_id,
                    matched = report.matched,
                    actions = report.actions_taken.len(),
                    errors = report.errors.len(),
                    "RunRuleNow applied selected-stack execution plan"
                ),
                Err(error) => {
                    tracing::warn!(
                        target: "bentodesk::rules",
                        rule_id = %rule_id,
                        error = %error,
                        "RunRuleNow failed"
                    );
                    set_rules_wizard_error(
                        root,
                        localized_current(
                            format!("运行失败：{error}"),
                            format!("Run failed: {error}"),
                        ),
                    );
                }
            }
            if let Some(target) = find_aux_window(root, WindowKind::RulesWizard) {
                request_redraw(target);
            }
            effects.needs_redraw = true;
        }
        _ => unreachable!("command routed to the wrong workflows dispatcher"),
    }
}
