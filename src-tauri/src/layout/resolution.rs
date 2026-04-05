//! Screen resolution detection and zone position clamping.
//!
//! When the user changes display resolution or DPI, zone positions that exceed
//! the new screen bounds are clamped so they remain visible.
//!
//! The [`start_resolution_monitor`] function spawns a background task that polls
//! for display changes and emits a `"resolution_changed"` Tauri event when the
//! primary monitor's resolution or DPI changes.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::persistence::{BentoZone, LayoutData};
use crate::AppState;

/// When set to `true`, the resolution monitor loop exits gracefully.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Signal the resolution monitor to stop polling and exit.
pub fn shutdown() {
    SHUTDOWN.store(true, Ordering::Release);
}

/// Screen resolution information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// Detect the current primary monitor resolution.
///
/// # Safety
/// `GetSystemMetrics` is a safe Win32 call that returns display dimensions.
pub fn get_current_resolution() -> Resolution {
    let width = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
        )
    };
    let height = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
        )
    };
    Resolution {
        width: width as u32,
        height: height as u32,
    }
}

/// Get the current DPI scale factor for the primary monitor.
///
/// Returns a multiplier (e.g. 1.0 = 96 DPI, 1.25 = 120 DPI, 1.5 = 144 DPI).
///
/// # Safety
/// `GetDpiForSystem` is a safe Win32 call that returns the system DPI value.
pub fn get_dpi_scale() -> f64 {
    // GetDpiForSystem is in Win32::UI::HiDpi in windows crate 0.58+
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() };
    dpi as f64 / 96.0
}

/// Payload emitted when the display resolution or DPI changes.
#[derive(Debug, Clone, Serialize)]
pub struct ResolutionChangedPayload {
    pub old_resolution: Resolution,
    pub new_resolution: Resolution,
    pub old_dpi: f64,
    pub new_dpi: f64,
}

/// Start a background task that polls for display resolution/DPI changes.
///
/// Checks every 2 seconds. When a change is detected:
/// 1. Clamps all zone positions to the new screen bounds.
/// 2. Persists the updated layout.
/// 3. Emits a `"resolution_changed"` event so the frontend can re-render.
pub fn start_resolution_monitor(handle: &AppHandle) {
    let handle = handle.clone();
    let mut last_res = get_current_resolution();
    let mut last_dpi = get_dpi_scale();

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));

            if SHUTDOWN.load(Ordering::Acquire) {
                tracing::info!("Resolution monitor shutting down");
                return;
            }

            let new_res = get_current_resolution();
            let new_dpi = get_dpi_scale();

            let res_changed = new_res.width != last_res.width || new_res.height != last_res.height;
            let dpi_changed = (new_dpi - last_dpi).abs() > f64::EPSILON;

            if res_changed || dpi_changed {
                tracing::info!(
                    "Display change detected: {}x{} @ {:.2}x -> {}x{} @ {:.2}x",
                    last_res.width, last_res.height, last_dpi,
                    new_res.width, new_res.height, new_dpi,
                );

                let payload = ResolutionChangedPayload {
                    old_resolution: last_res.clone(),
                    new_resolution: new_res.clone(),
                    old_dpi: last_dpi,
                    new_dpi,
                };

                // Clamp zones to the new screen bounds and persist.
                if let Some(state) = handle.try_state::<AppState>() {
                    if let Ok(mut layout) = state.layout.lock() {
                        clamp_zones_to_screen(&mut layout);
                        layout.last_modified = chrono::Utc::now().to_rfc3339();
                    }
                    state.persist_layout();
                }

                // Reposition the overlay window to cover the new work area.
                crate::ghost_layer::manager::reposition_to_work_area();

                if let Err(e) = handle.emit("resolution_changed", &payload) {
                    tracing::warn!("Failed to emit resolution_changed event: {}", e);
                }

                last_res = new_res;
                last_dpi = new_dpi;
            }
        }
    });
}

/// Convert a relative X percentage to absolute pixel value.
pub fn relative_x_to_pixels(x_percent: f64, screen_width: u32) -> f64 {
    x_percent / 100.0 * screen_width as f64
}

/// Convert a relative Y percentage to absolute pixel value.
pub fn relative_y_to_pixels(y_percent: f64, screen_height: u32) -> f64 {
    y_percent / 100.0 * screen_height as f64
}

/// Convert absolute pixels to relative X percentage.
pub fn pixels_to_relative_x(pixels: f64, screen_width: u32) -> f64 {
    pixels / screen_width as f64 * 100.0
}

/// Convert absolute pixels to relative Y percentage.
pub fn pixels_to_relative_y(pixels: f64, screen_height: u32) -> f64 {
    pixels / screen_height as f64 * 100.0
}

/// Clamp all zones in a layout so they remain within the visible screen bounds.
///
/// Zone positions (percentage-based) are clamped to `[0.0, 100.0 - expanded_size]` so
/// that the zone plus its expanded dimensions fit on screen.
pub fn clamp_zones_to_screen(layout: &mut LayoutData) {
    for zone in &mut layout.zones {
        clamp_zone(zone);
    }
}

/// Clamp a single zone's position to valid screen bounds.
fn clamp_zone(zone: &mut BentoZone) {
    // Ensure the zone's top-left + size doesn't exceed 100%
    let max_x = (100.0 - zone.expanded_size.w_percent).max(0.0);
    let max_y = (100.0 - zone.expanded_size.h_percent).max(0.0);

    zone.position.x_percent = zone.position.x_percent.clamp(0.0, max_x);
    zone.position.y_percent = zone.position.y_percent.clamp(0.0, max_y);

    // Also clamp expanded_size to reasonable bounds
    zone.expanded_size.w_percent = zone.expanded_size.w_percent.clamp(5.0, 100.0);
    zone.expanded_size.h_percent = zone.expanded_size.h_percent.clamp(5.0, 100.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{RelativePosition, RelativeSize};

    #[test]
    fn relative_x_to_pixels_correct() {
        assert!((relative_x_to_pixels(50.0, 1920) - 960.0).abs() < f64::EPSILON);
        assert!((relative_x_to_pixels(0.0, 1920) - 0.0).abs() < f64::EPSILON);
        assert!((relative_x_to_pixels(100.0, 1920) - 1920.0).abs() < f64::EPSILON);
    }

    #[test]
    fn relative_y_to_pixels_correct() {
        assert!((relative_y_to_pixels(50.0, 1080) - 540.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pixels_to_relative_roundtrip() {
        let x_pct = 37.5;
        let screen_w = 1920;
        let pixels = relative_x_to_pixels(x_pct, screen_w);
        let back = pixels_to_relative_x(pixels, screen_w);
        assert!((back - x_pct).abs() < f64::EPSILON);
    }

    #[test]
    fn pixels_to_relative_y_roundtrip() {
        let y_pct = 62.5;
        let screen_h = 1080;
        let pixels = relative_y_to_pixels(y_pct, screen_h);
        let back = pixels_to_relative_y(pixels, screen_h);
        assert!((back - y_pct).abs() < f64::EPSILON);
    }

    fn make_zone(x: f64, y: f64, w: f64, h: f64) -> BentoZone {
        BentoZone {
            id: "z1".to_string(),
            name: "Test".to_string(),
            icon: "T".to_string(),
            position: RelativePosition {
                x_percent: x,
                y_percent: y,
            },
            expanded_size: RelativeSize {
                w_percent: w,
                h_percent: h,
            },
            items: Vec::new(),
            accent_color: None,
            sort_order: 0,
            auto_group: None,
            grid_columns: 4,
            capsule_size: "medium".to_string(),
            capsule_shape: "pill".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn clamp_zone_keeps_valid_position() {
        let mut zone = make_zone(10.0, 20.0, 30.0, 40.0);
        clamp_zone(&mut zone);
        assert!((zone.position.x_percent - 10.0).abs() < f64::EPSILON);
        assert!((zone.position.y_percent - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clamp_zone_clamps_overflow_position() {
        let mut zone = make_zone(90.0, 85.0, 30.0, 40.0);
        clamp_zone(&mut zone);
        // max_x = 100 - 30 = 70, so x should be clamped to 70
        assert!((zone.position.x_percent - 70.0).abs() < f64::EPSILON);
        // max_y = 100 - 40 = 60, so y should be clamped to 60
        assert!((zone.position.y_percent - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clamp_zone_clamps_negative_position() {
        let mut zone = make_zone(-5.0, -10.0, 20.0, 20.0);
        clamp_zone(&mut zone);
        assert!((zone.position.x_percent - 0.0).abs() < f64::EPSILON);
        assert!((zone.position.y_percent - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clamp_zone_clamps_size_to_minimum() {
        let mut zone = make_zone(0.0, 0.0, 1.0, 2.0);
        clamp_zone(&mut zone);
        assert!((zone.expanded_size.w_percent - 5.0).abs() < f64::EPSILON);
        assert!((zone.expanded_size.h_percent - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clamp_zones_to_screen_applies_to_all() {
        let mut layout = LayoutData {
            version: "1.0.0".to_string(),
            zones: vec![
                make_zone(110.0, 110.0, 20.0, 20.0),
                make_zone(-5.0, -5.0, 50.0, 50.0),
            ],
            last_modified: String::new(),
            coherence_id: None,
        };
        clamp_zones_to_screen(&mut layout);
        assert!((layout.zones[0].position.x_percent - 80.0).abs() < f64::EPSILON);
        assert!((layout.zones[0].position.y_percent - 80.0).abs() < f64::EPSILON);
        assert!((layout.zones[1].position.x_percent - 0.0).abs() < f64::EPSILON);
        assert!((layout.zones[1].position.y_percent - 0.0).abs() < f64::EPSILON);
    }
}
