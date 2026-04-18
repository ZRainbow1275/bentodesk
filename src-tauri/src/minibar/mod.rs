//! Mini Bar — Pin a zone as a floating always-on-top toolbar.
//!
//! Each minibar is an independent Tauri `WebviewWindow` carrying the
//! `?minibar={zone_id}` query string. The frontend inspects that param at
//! startup and renders the lightweight `MiniBarView` in place of the full UI.
//!
//! **Hard limit**: at most 3 concurrent minibars. Each Tauri window spawns its
//! own WebView2 process, so the cost is linear in windows (~50 MB each).

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::BentoDeskError;

const MAX_ACTIVE_MINIBARS: usize = 3;

const WIN_WIDTH: f64 = 240.0;
const WIN_HEIGHT: f64 = 56.0;

/// Thin registry of labels for the spawned minibar windows. Only used to
/// enforce the 3-window cap and to answer `list_pinned_minibars`.
#[derive(Default)]
struct Registry {
    labels: Vec<String>,
}

static REG: Mutex<Registry> = Mutex::new(Registry { labels: Vec::new() });

fn label_for_zone(zone_id: &str) -> String {
    // Keep ASCII-only to satisfy Tauri's label rules.
    let safe: String = zone_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("minibar-{safe}")
}

/// Create a minibar window for the given zone. Returns the Tauri window label
/// so the frontend can reference it later.
pub fn pin_zone(handle: &AppHandle, zone_id: &str) -> Result<String, BentoDeskError> {
    {
        let reg = REG
            .lock()
            .map_err(|e| BentoDeskError::Generic(e.to_string()))?;
        if reg.labels.len() >= MAX_ACTIVE_MINIBARS {
            return Err(BentoDeskError::Generic(format!(
                "At most {MAX_ACTIVE_MINIBARS} minibars can be active. Close one first."
            )));
        }
        if reg.labels.contains(&label_for_zone(zone_id)) {
            return Ok(label_for_zone(zone_id));
        }
    }

    let label = label_for_zone(zone_id);
    let url = WebviewUrl::App(format!("index.html?minibar={zone_id}").into());

    let builder = WebviewWindowBuilder::new(handle, &label, url)
        .title("BentoDesk")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .inner_size(WIN_WIDTH, WIN_HEIGHT)
        .resizable(false)
        .shadow(false);

    let window = builder
        .build()
        .map_err(|e| BentoDeskError::Generic(format!("Failed to create minibar window: {e}")))?;

    // Tool-window style so Alt-Tab skips it.
    #[cfg(windows)]
    apply_toolwindow_style(&window);
    // Silence unused on non-Windows (minibar is Windows-first, but
    // we compile on CI too).
    #[cfg(not(windows))]
    let _ = &window;

    let _ = window.set_size(LogicalSize::new(WIN_WIDTH, WIN_HEIGHT));

    {
        let mut reg = REG
            .lock()
            .map_err(|e| BentoDeskError::Generic(e.to_string()))?;
        if !reg.labels.contains(&label) {
            reg.labels.push(label.clone());
        }
    }

    tracing::info!("Minibar spawned for zone {} (label={})", zone_id, label);
    Ok(label)
}

/// Close a minibar by label.
pub fn unpin(handle: &AppHandle, label: &str) -> Result<(), BentoDeskError> {
    if let Some(win) = handle.get_webview_window(label) {
        let _ = win.close();
    }
    let mut reg = REG
        .lock()
        .map_err(|e| BentoDeskError::Generic(e.to_string()))?;
    reg.labels.retain(|l| l != label);
    Ok(())
}

/// Snapshot of currently pinned minibar labels.
pub fn list() -> Vec<String> {
    REG.lock().map(|r| r.labels.clone()).unwrap_or_default()
}

/// Apply `WS_EX_TOOLWINDOW` so the minibar doesn't appear in Alt-Tab.
#[cfg(windows)]
fn apply_toolwindow_style(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    let hwnd_raw = match window.hwnd() {
        Ok(h) => h.0,
        Err(e) => {
            tracing::warn!("minibar: hwnd() failed — cannot apply toolwindow style: {e}");
            return;
        }
    };
    unsafe {
        let hwnd = HWND(hwnd_raw);
        let old = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new = (old | WS_EX_TOOLWINDOW.0 as i32) & !(WS_EX_APPWINDOW.0 as i32);
        SetWindowLongW(hwnd, GWL_EXSTYLE, new);
    }
}

/// Request payload to forward a minibar-triggered item launch to the main window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinibarLaunchItem {
    pub zone_id: String,
    pub item_id: String,
    pub path: String,
}
