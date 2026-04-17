//! Timeline IPC commands — list / restore / delete / pin checkpoints.

use tauri::{AppHandle, Manager, State};

use crate::timeline::checkpoint::{
    self, Checkpoint, CheckpointMeta, CheckpointStore, DeltaSummary,
};
use crate::timeline::hook;
use crate::AppState;

/// List all checkpoints (auto + pinned), sorted by capture time ascending.
#[tauri::command]
pub async fn list_checkpoints(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CheckpointMeta>, String> {
    // Lazily reload in case background writes landed since startup.
    let store = CheckpointStore::new(hook::timeline_dir(&app));
    let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
    buf.reload(&store);
    Ok(buf.metas())
}

/// Fetch a single checkpoint's full payload (including zones) for preview.
#[tauri::command]
pub async fn get_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Checkpoint, String> {
    let store = CheckpointStore::new(hook::timeline_dir(&app));
    // Prefer in-memory cache to avoid disk I/O on hover.
    {
        let buf = state.timeline.lock().map_err(|e| e.to_string())?;
        if let Some(cp) = buf.merged().iter().find(|c| c.id == id) {
            return Ok((*cp).clone());
        }
    }
    store.load(&id).map_err(|e| e.to_string())
}

/// Restore the layout to a specific checkpoint. Also captures a "pre-restore"
/// checkpoint so the user can undo the undo.
#[tauri::command]
pub async fn restore_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let store = CheckpointStore::new(hook::timeline_dir(&app));
    let target = {
        let buf = state.timeline.lock().map_err(|e| e.to_string())?;
        buf.merged()
            .iter()
            .find(|c| c.id == id)
            .map(|c| (*c).clone())
    };
    let target = match target {
        Some(t) => t,
        None => store.load(&id).map_err(|e| e.to_string())?,
    };

    // Capture current state as a "pre-restore" pinned entry if it would
    // otherwise vanish (e.g. user is about to overwrite the most recent auto).
    // We keep this lightweight: just a normal auto checkpoint, not pinned.
    {
        let pre_cp = Checkpoint {
            id: checkpoint::new_checkpoint_id(),
            snapshot: capture_current(&app),
            delta: DeltaSummary::default(),
            delta_summary: "pre-restore".to_string(),
            trigger: "pre_restore".to_string(),
            pinned: false,
        };
        let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
        buf.push_auto(&store, pre_cp);
    }

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        layout.zones = target.snapshot.zones.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();

    // Advance the cursor to the restored point so redo works intuitively.
    {
        let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
        buf.seek(&id);
    }

    // Reset the hook baseline so the next change is diffed against the restored point.
    hook::init_baseline(&app);

    Ok(())
}

/// Undo — restore the previous checkpoint relative to the current cursor.
#[tauri::command]
pub async fn undo_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let target = {
        let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
        buf.step_back()
    };
    let target = match target {
        Some(t) => t,
        None => return Ok(None),
    };
    let id = target.id.clone();

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        layout.zones = target.snapshot.zones.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    hook::init_baseline(&app);
    Ok(Some(id))
}

/// Redo — step the cursor forward.
#[tauri::command]
pub async fn redo_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let target = {
        let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
        buf.step_forward()
    };
    let target = match target {
        Some(t) => t,
        None => return Ok(None),
    };
    let id = target.id.clone();

    {
        let mut layout = state.layout.lock().map_err(|e| e.to_string())?;
        layout.zones = target.snapshot.zones.clone();
        layout.last_modified = chrono::Utc::now().to_rfc3339();
    }
    state.persist_layout();
    hook::init_baseline(&app);
    Ok(Some(id))
}

/// Delete a checkpoint.
#[tauri::command]
pub async fn delete_checkpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let store = CheckpointStore::new(hook::timeline_dir(&app));
    let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;
    buf.remove(&store, &id);
    Ok(())
}

/// Promote an auto checkpoint to permanent (pinned). If no such checkpoint
/// exists, create a fresh one from the current layout.
#[tauri::command]
pub async fn save_checkpoint_permanent(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Option<String>,
    label: Option<String>,
) -> Result<CheckpointMeta, String> {
    let store = CheckpointStore::new(hook::timeline_dir(&app));
    let mut buf = state.timeline.lock().map_err(|e| e.to_string())?;

    let cp = if let Some(id) = id {
        match buf.pin(&store, &id) {
            Some(cp) => cp,
            None => return Err(format!("Checkpoint not found: {id}")),
        }
    } else {
        let snap = capture_current(&app);
        let summary = label
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "manual save".to_string());
        let cp = Checkpoint {
            id: checkpoint::new_checkpoint_id(),
            snapshot: snap,
            delta: DeltaSummary::default(),
            delta_summary: summary,
            trigger: "manual".to_string(),
            pinned: true,
        };
        buf.push_pinned(&store, cp.clone());
        cp
    };

    Ok((&cp).into())
}

fn capture_current(app: &AppHandle) -> crate::layout::snapshot::DesktopSnapshot {
    let state = app.state::<crate::AppState>();
    let layout: crate::layout::persistence::LayoutData = match state.layout.lock() {
        Ok(l) => l.clone(),
        Err(_) => crate::layout::persistence::LayoutData::default(),
    };
    let res = crate::layout::resolution::get_current_resolution();
    let dpi = crate::layout::resolution::get_dpi_scale();
    crate::layout::snapshot::DesktopSnapshot {
        id: uuid::Uuid::new_v4().to_string(),
        name: String::new(),
        resolution: res,
        dpi,
        zones: layout.zones,
        captured_at: chrono::Utc::now().to_rfc3339(),
    }
}
