//! Context Capsule — save and restore a "workflow" snapshot of open windows.
//!
//! A capsule captures the bounding rect + title + class + process of every
//! visible top-level window, then re-applies those rects on restore by
//! matching captured records back to currently running HWNDs.
//!
//! Storage: `%APPDATA%/BentoDesk/capsules/{id}.json`, one file per capsule.

pub mod enum_windows;
pub mod matcher;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE,
};

use crate::error::BentoDeskError;

/// Single window record within a capsule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedWindow {
    pub title: String,
    pub class_name: String,
    pub process_name: String,
    /// `(left, top, right, bottom)` in screen coordinates at capture time.
    pub rect: (i32, i32, i32, i32),
    pub is_maximized: bool,
}

/// A named capsule — the persisted unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCapsule {
    pub id: String,
    pub name: String,
    /// Display icon (follows the `ZoneIcon` namespace system).
    pub icon: String,
    pub captured_at: String,
    pub windows: Vec<CapturedWindow>,
}

/// Per-capsule restore statistics surfaced to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestoreResult {
    /// Captured windows matched + repositioned.
    pub restored: Vec<String>,
    /// Captured windows with no live match (user needs to launch the app).
    pub pending: Vec<String>,
    /// Errors returned by `SetWindowPos` (UAC / admin windows).
    pub errors: Vec<String>,
}

static LOCK: Mutex<()> = Mutex::new(());

fn capsules_dir(handle: &AppHandle) -> PathBuf {
    let base = crate::storage::state_data_dir(handle);
    let dir = base.join("capsules");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

fn capsule_path(handle: &AppHandle, id: &str) -> PathBuf {
    capsules_dir(handle).join(format!("{id}.json"))
}

/// Capture every visible top-level window into a new capsule.
pub fn capture(
    handle: &AppHandle,
    name: String,
    icon: Option<String>,
) -> Result<ContextCapsule, BentoDeskError> {
    let _g = LOCK.lock().ok();

    let live = enum_windows::enumerate_windows();
    let windows: Vec<CapturedWindow> = live
        .into_iter()
        .map(|lw| CapturedWindow {
            title: lw.title,
            class_name: lw.class_name,
            process_name: lw.process_name,
            rect: lw.rect,
            is_maximized: lw.is_maximized,
        })
        .collect();

    let capsule = ContextCapsule {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        icon: icon.unwrap_or_else(|| "briefcase".to_string()),
        captured_at: chrono::Utc::now().to_rfc3339(),
        windows,
    };

    let path = capsule_path(handle, &capsule.id);
    fs::write(&path, serde_json::to_vec_pretty(&capsule)?)?;
    tracing::info!(
        "Captured context capsule '{}' with {} windows",
        capsule.name,
        capsule.windows.len()
    );
    Ok(capsule)
}

/// List every persisted capsule, most-recent first.
pub fn list(handle: &AppHandle) -> Vec<ContextCapsule> {
    let dir = capsules_dir(handle);
    let mut out = Vec::new();
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(txt) = fs::read_to_string(&path) {
            if let Ok(cap) = serde_json::from_str::<ContextCapsule>(&txt) {
                out.push(cap);
            }
        }
    }
    out.sort_by(|a, b| b.captured_at.cmp(&a.captured_at));
    out
}

/// Delete a capsule by id. Returns `Ok` even when the id is unknown.
pub fn delete(handle: &AppHandle, id: &str) -> Result<(), BentoDeskError> {
    let _g = LOCK.lock().ok();
    let path = capsule_path(handle, id);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Restore a capsule — match captured windows to live HWNDs and reposition.
pub fn restore(handle: &AppHandle, id: &str) -> Result<RestoreResult, BentoDeskError> {
    let path = capsule_path(handle, id);
    let text = fs::read_to_string(&path)?;
    let cap: ContextCapsule = serde_json::from_str(&text)?;

    let live = enum_windows::enumerate_windows();
    let mut result = RestoreResult::default();

    for win in &cap.windows {
        let hwnd = match matcher::match_window(win, &live) {
            Some(h) => h,
            None => {
                result.pending.push(win.title.clone());
                continue;
            }
        };

        let (l, t, r, b) = win.rect;
        let w = (r - l).max(50);
        let h = (b - t).max(50);

        unsafe {
            let hwnd_wrap = HWND(hwnd as *mut core::ffi::c_void);

            if win.is_maximized {
                let _ = ShowWindow(hwnd_wrap, SW_MAXIMIZE);
                result.restored.push(win.title.clone());
                continue;
            }

            let _ = ShowWindow(hwnd_wrap, SW_RESTORE);
            match SetWindowPos(
                hwnd_wrap,
                HWND_TOP,
                l,
                t,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) {
                Ok(()) => result.restored.push(win.title.clone()),
                Err(e) => result.errors.push(format!("{}: {}", win.title, e)),
            }
        }
    }

    tracing::info!(
        "Restored capsule '{}': {} ok, {} pending, {} errors",
        cap.name,
        result.restored.len(),
        result.pending.len(),
        result.errors.len()
    );
    Ok(result)
}
