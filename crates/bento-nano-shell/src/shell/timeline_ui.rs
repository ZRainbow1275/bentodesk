//! Native shell owner: `timeline_ui`.

use super::*;

#[derive(Debug)]
pub(super) enum TimelineError {
    MissingStateDir,
    EmptyCheckpointId,
    CheckpointNotFound(SmolStr),
    Checkpoint(CheckpointError),
}

impl core::fmt::Display for TimelineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingStateDir => f.write_str("timeline state directory is unavailable"),
            Self::EmptyCheckpointId => f.write_str("checkpoint id is empty"),
            Self::CheckpointNotFound(id) => write!(f, "checkpoint not found: {id}"),
            Self::Checkpoint(source) => write!(f, "checkpoint store failed: {source}"),
        }
    }
}

impl core::error::Error for TimelineError {}

impl From<CheckpointError> for TimelineError {
    fn from(value: CheckpointError) -> Self {
        Self::Checkpoint(value)
    }
}

#[derive(Debug)]
pub(super) enum SnapshotPickerError {
    MissingStateDir,
    EmptySnapshotId,
    SnapshotNotFound(SmolStr),
    Layout(LayoutError),
}

impl core::fmt::Display for SnapshotPickerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingStateDir => f.write_str("snapshot state directory is unavailable"),
            Self::EmptySnapshotId => f.write_str("snapshot id is empty"),
            Self::SnapshotNotFound(id) => write!(f, "snapshot not found: {id}"),
            Self::Layout(source) => write!(f, "snapshot store failed: {source}"),
        }
    }
}

impl core::error::Error for SnapshotPickerError {}

impl From<LayoutError> for SnapshotPickerError {
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

pub(super) fn timeline_dir_for_zones_path(zones_path: &Path) -> Result<PathBuf, TimelineError> {
    let Some(parent) = zones_path.parent() else {
        return Err(TimelineError::MissingStateDir);
    };
    Ok(parent.join("timeline"))
}

pub(super) fn snapshot_dir_for_zones_path(
    zones_path: &Path,
) -> Result<PathBuf, SnapshotPickerError> {
    let Some(parent) = zones_path.parent() else {
        return Err(SnapshotPickerError::MissingStateDir);
    };
    Ok(parent.join("snapshots"))
}

pub(super) fn current_snapshot_manager(
    root: &AppRoot,
) -> Result<SnapshotManager, SnapshotPickerError> {
    let zones_path = root.app.borrow().zones_path.clone();
    if !zones_path.as_os_str().is_empty() {
        return Ok(SnapshotManager::new(snapshot_dir_for_zones_path(
            &zones_path,
        )?));
    }
    let fallback = storage::appdata_path().map_err(|_| SnapshotPickerError::MissingStateDir)?;
    Ok(SnapshotManager::new(snapshot_dir_for_zones_path(
        &fallback,
    )?))
}

pub(super) fn current_timeline_store(root: &AppRoot) -> Result<CheckpointStore, TimelineError> {
    let zones_path = root.app.borrow().zones_path.clone();
    if !zones_path.as_os_str().is_empty() {
        return Ok(CheckpointStore::new(timeline_dir_for_zones_path(
            &zones_path,
        )?));
    }
    let fallback = storage::appdata_path().map_err(|_| TimelineError::MissingStateDir)?;
    Ok(CheckpointStore::new(timeline_dir_for_zones_path(
        &fallback,
    )?))
}

pub(super) fn selected_timeline_checkpoint_id(root: &AppRoot) -> Option<SmolStr> {
    let app = root.app.borrow();
    app.timeline_panel.borrow().selected_id()
}

pub(super) fn selected_snapshot_id(root: &AppRoot) -> Option<SmolStr> {
    let app = root.app.borrow();
    app.snapshot_picker.borrow().selected_id()
}

pub(super) fn set_timeline_error(root: &AppRoot, error: SmolStr) {
    let app = root.app.borrow();
    app.timeline_panel.borrow_mut().set_error(error);
}

pub(super) fn set_timeline_status(root: &AppRoot, status: SmolStr) {
    let app = root.app.borrow();
    app.timeline_panel.borrow_mut().set_status(status);
}

pub(super) fn set_snapshot_picker_error(root: &AppRoot, error: SmolStr) {
    let app = root.app.borrow();
    app.snapshot_picker.borrow_mut().set_error(error);
}

pub(super) fn set_snapshot_picker_status(root: &AppRoot, status: SmolStr) {
    let app = root.app.borrow();
    app.snapshot_picker.borrow_mut().set_status(status);
}

pub(super) fn refresh_timeline_panel(root: &AppRoot) -> Result<(), TimelineError> {
    let store = current_timeline_store(root)?;
    root.timeline_buffer.borrow_mut().reload(&store);
    sync_timeline_panel_from_buffer(root)
}

pub(super) fn sync_timeline_panel_from_buffer(root: &AppRoot) -> Result<(), TimelineError> {
    let entries = root.timeline_buffer.borrow().metas();
    {
        let app = root.app.borrow();
        let mut panel = app.timeline_panel.borrow_mut();
        panel.set_entries(entries);
        panel.clear_status();
    }
    load_selected_timeline_checkpoint(root)
}

pub(super) fn load_selected_timeline_checkpoint(root: &AppRoot) -> Result<(), TimelineError> {
    let Some(checkpoint_id) = selected_timeline_checkpoint_id(root) else {
        let app = root.app.borrow();
        app.timeline_panel.borrow_mut().set_active(None);
        return Ok(());
    };
    let checkpoint = load_timeline_checkpoint(root, checkpoint_id.as_str())?;
    let app = root.app.borrow();
    app.timeline_panel.borrow_mut().set_active(Some(checkpoint));
    Ok(())
}

pub(super) fn select_timeline_checkpoint(root: &AppRoot, index: usize) {
    let changed = {
        let app = root.app.borrow();
        app.timeline_panel.borrow_mut().select_index(index)
    };
    if changed {
        if let Err(error) = load_selected_timeline_checkpoint(root) {
            set_timeline_error(
                root,
                localized_current(
                    format!("预览失败：{error}"),
                    format!("Preview failed: {error}"),
                ),
            );
        }
    }
}

pub(super) fn select_prev_timeline_checkpoint(root: &AppRoot) {
    {
        let app = root.app.borrow();
        app.timeline_panel.borrow_mut().select_prev();
    }
    if let Err(error) = load_selected_timeline_checkpoint(root) {
        set_timeline_error(
            root,
            localized_current(
                format!("预览失败：{error}"),
                format!("Preview failed: {error}"),
            ),
        );
    }
}

pub(super) fn select_next_timeline_checkpoint(root: &AppRoot) {
    {
        let app = root.app.borrow();
        app.timeline_panel.borrow_mut().select_next();
    }
    if let Err(error) = load_selected_timeline_checkpoint(root) {
        set_timeline_error(
            root,
            localized_current(
                format!("预览失败：{error}"),
                format!("Preview failed: {error}"),
            ),
        );
    }
}

pub(super) fn load_timeline_checkpoint(
    root: &AppRoot,
    checkpoint_id: &str,
) -> Result<Checkpoint, TimelineError> {
    if checkpoint_id.trim().is_empty() {
        return Err(TimelineError::EmptyCheckpointId);
    }
    {
        let buffer = root.timeline_buffer.borrow();
        if let Some(checkpoint) = buffer
            .merged()
            .iter()
            .find(|checkpoint| checkpoint.id.as_str() == checkpoint_id)
        {
            return Ok((*checkpoint).clone());
        }
    }
    let store = current_timeline_store(root)?;
    Ok(store.load(checkpoint_id)?)
}

pub(super) fn ensure_timeline_loaded(root: &AppRoot, store: &CheckpointStore) {
    let should_reload = root.timeline_buffer.borrow().merged().is_empty();
    if should_reload {
        root.timeline_buffer.borrow_mut().reload(store);
    }
}

pub(super) fn open_timeline(root: &AppRoot) {
    use bento_nano_app::business::timeline as timeline_ui;
    let _panel = timeline_ui::build_timeline_panel();
    if let Err(error) = refresh_timeline_panel(root) {
        tracing::warn!(
            target: "bentodesk::timeline",
            error = %error,
            "OpenTimeline: checkpoint list refresh failed"
        );
        set_timeline_error(
            root,
            localized_current(
                format!("时间线列表载入失败：{error}"),
                format!("Timeline list failed: {error}"),
            ),
        );
    }

    let Some(host) = ensure_aux_window(root, WindowKind::Timeline) else {
        tracing::warn!(
            target: "bentodesk::timeline",
            "OpenTimeline: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: ShowWindow + SetForegroundWindow canonical for a focusable aux.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    tracing::info!(
        target: "bentodesk::timeline",
        "OpenTimeline — selected-stack checkpoint save/list/restore/undo/redo/delete ready"
    );
}

pub(super) fn refresh_snapshot_picker(root: &AppRoot) -> Result<(), SnapshotPickerError> {
    let manager = current_snapshot_manager(root)?;
    let snapshots = manager.list()?;
    let app = root.app.borrow();
    let mut picker = app.snapshot_picker.borrow_mut();
    picker.set_entries(snapshots);
    picker.clear_status();
    Ok(())
}

pub(super) fn open_snapshot_picker(root: &AppRoot) {
    use bento_nano_app::business::timeline as timeline_ui;
    let _picker = timeline_ui::build_snapshot_picker();
    if let Err(error) = refresh_snapshot_picker(root) {
        tracing::warn!(
            target: "bentodesk::snapshot",
            error = %error,
            "OpenSnapshotPicker: snapshot list refresh failed"
        );
        set_snapshot_picker_error(
            root,
            localized_current(
                format!("布局快照列表载入失败：{error}"),
                format!("Snapshot list failed: {error}"),
            ),
        );
    }

    let Some(host) = ensure_aux_window(root, WindowKind::SnapshotPicker) else {
        tracing::warn!(
            target: "bentodesk::snapshot",
            "OpenSnapshotPicker: ensure_aux_window failed"
        );
        return;
    };

    // SAFETY: ShowWindow + SetForegroundWindow canonical for a focusable aux.
    unsafe {
        ShowWindow(host, SW_SHOW);
        SetForegroundWindow(host);
    }
    tracing::info!(
        target: "bentodesk::snapshot",
        "OpenSnapshotPicker — selected-stack save/load/delete ready"
    );
}

pub(super) fn snapshot_capture_name(root: &AppRoot) -> SmolStr {
    let zone_count = root.app.borrow().zones.len();
    localized_current(
        format!(
            "手动快照 · {zone_count} 个区域 · {}",
            bento_nano_backend::time::now_rfc3339()
        ),
        format!(
            "Manual snapshot · {zone_count} zones · {}",
            bento_nano_backend::time::now_rfc3339()
        ),
    )
}

pub(super) fn resolve_snapshot_id(
    manager: &SnapshotManager,
    snapshot_id: &str,
) -> Result<SmolStr, SnapshotPickerError> {
    if snapshot_id.trim().is_empty() {
        return Err(SnapshotPickerError::EmptySnapshotId);
    }
    manager
        .list()?
        .into_iter()
        .find(|snapshot| snapshot.id.as_str() == snapshot_id)
        .map(|snapshot| snapshot.id)
        .ok_or_else(|| SnapshotPickerError::SnapshotNotFound(SmolStr::new(snapshot_id)))
}

pub(super) fn save_layout_snapshot(
    root: &AppRoot,
    name: Option<SmolStr>,
) -> Result<DesktopSnapshot, SnapshotPickerError> {
    let manager = current_snapshot_manager(root)?;
    let name = name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| snapshot_capture_name(root));
    let snapshot = capture_current_timeline_snapshot(root, name.as_str());
    manager.save(&snapshot)?;
    refresh_snapshot_picker(root)?;
    set_snapshot_picker_status(
        root,
        SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                "已保存当前布局快照".to_owned()
            } else {
                format!("Saved snapshot {}", snapshot.id)
            },
        ),
    );
    Ok(snapshot)
}

pub(super) fn load_layout_snapshot(
    root: &AppRoot,
    snapshot_id: &str,
) -> Result<DesktopSnapshot, SnapshotPickerError> {
    let manager = current_snapshot_manager(root)?;
    let stable_id = resolve_snapshot_id(&manager, snapshot_id)?;
    let snapshot = manager.load(stable_id.as_str())?;
    {
        let mut app = root.app.borrow_mut();
        app.zones = zone_list_from_bento_zones(&snapshot.zones, app.viewport);
        bump_next_zone_id_from_zones(&app);
        app.mark_dirty();
    }
    refresh_snapshot_picker(root)?;
    set_snapshot_picker_status(
        root,
        SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                "已载入布局快照".to_owned()
            } else {
                format!("Loaded snapshot {}", snapshot.id)
            },
        ),
    );
    Ok(snapshot)
}

pub(super) fn delete_layout_snapshot(
    root: &AppRoot,
    snapshot_id: &str,
) -> Result<(), SnapshotPickerError> {
    let manager = current_snapshot_manager(root)?;
    let stable_id = resolve_snapshot_id(&manager, snapshot_id)?;
    manager.delete(stable_id.as_str())?;
    refresh_snapshot_picker(root)?;
    set_snapshot_picker_status(
        root,
        SmolStr::new(
            if bento_nano_style::current_locale_is(&bento_nano_style::ZH_CN) {
                "已删除布局快照".to_owned()
            } else {
                format!("Deleted snapshot {stable_id}")
            },
        ),
    );
    Ok(())
}
