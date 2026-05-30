//! T-097 — screen resolution detection + zone clamping + display monitor.
//!
//! Replaces 1.x's Tauri-coupled `start_resolution_monitor` with a
//! channel-based variant that emits [`ResolutionChangedPayload`] on a
//! `crossbeam_channel::Sender`. The dispatcher routes the payload to the
//! ghost-layer reposition + the layout persist pass — those side effects
//! used to live inside the monitor itself.

use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use super::persistence::{BentoZone, LayoutData};

/// When set to `true`, the resolution monitor loop exits gracefully.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Signal the resolution monitor to stop polling and exit. Safe to call from
/// any thread; the monitor returns within the next 2-second poll cycle.
pub fn shutdown_resolution_monitor() {
    SHUTDOWN.store(true, Ordering::Release);
}

/// Screen resolution information.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

/// Detect the current primary monitor resolution.
pub fn get_current_resolution() -> Resolution {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    // SAFETY: GetSystemMetrics is a documented Win32 call that returns the
    // current primary-monitor pixel dimensions. It cannot fail in any way
    // visible to the caller — at worst it returns 0 if the screen object
    // hasn't initialised yet, and we surface that as Resolution { 0, 0 }.
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    Resolution {
        width: width.max(0) as u32,
        height: height.max(0) as u32,
    }
}

/// Get the current DPI scale factor for the primary monitor.
///
/// Returns a multiplier (e.g. 1.0 = 96 DPI, 1.25 = 120 DPI, 1.5 = 144 DPI).
pub fn get_dpi_scale() -> f64 {
    // Mc-1a — DPI soft-loaded via `crate::dpi_compat` (GetProcAddress) so the
    // backend carries no static `GetDpiForSystem` import (absent on Win10
    // <1607 / 8.1 / 7 → EXE load failure).
    let dpi = crate::dpi_compat::system_dpi();
    f64::from(dpi) / 96.0
}

/// Payload emitted when the display resolution or DPI changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ResolutionChangedPayload {
    pub old_resolution: Resolution,
    pub new_resolution: Resolution,
    pub old_dpi: f64,
    pub new_dpi: f64,
}

/// Start a background `std::thread` that polls for display resolution / DPI
/// changes every 2 seconds. Emits [`ResolutionChangedPayload`] on `event_tx`
/// when a change is detected.
///
/// The dispatcher is responsible for:
/// 1. Calling [`clamp_zones_to_screen`] on the live `LayoutData` snapshot.
/// 2. Persisting the clamped layout (touch + save).
/// 3. Repositioning the ghost-layer overlay to the new work area.
///
/// This module deliberately does NOT do those side effects — they require
/// access to live state owned by other crates and are easier to test/mock at
/// the dispatcher level.
pub fn start_resolution_monitor(event_tx: Sender<ResolutionChangedPayload>) {
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

            let res_changed = new_res != last_res;
            let dpi_changed = (new_dpi - last_dpi).abs() > f64::EPSILON;

            if res_changed || dpi_changed {
                tracing::info!(
                    "Display change detected: {}x{} @ {:.2}x -> {}x{} @ {:.2}x",
                    last_res.width,
                    last_res.height,
                    last_dpi,
                    new_res.width,
                    new_res.height,
                    new_dpi,
                );

                let payload = ResolutionChangedPayload {
                    old_resolution: last_res,
                    new_resolution: new_res,
                    old_dpi: last_dpi,
                    new_dpi,
                };

                if event_tx.send(payload).is_err() {
                    tracing::warn!("resolution monitor: event channel closed, exiting");
                    return;
                }

                last_res = new_res;
                last_dpi = new_dpi;
            }
        }
    });
}

/// Convert a relative X percentage to absolute pixel value.
pub fn relative_x_to_pixels(x_percent: f64, screen_width: u32) -> f64 {
    x_percent / 100.0 * f64::from(screen_width)
}

/// Convert a relative Y percentage to absolute pixel value.
pub fn relative_y_to_pixels(y_percent: f64, screen_height: u32) -> f64 {
    y_percent / 100.0 * f64::from(screen_height)
}

/// Convert absolute pixels to relative X percentage.
pub fn pixels_to_relative_x(pixels: f64, screen_width: u32) -> f64 {
    pixels / f64::from(screen_width) * 100.0
}

/// Convert absolute pixels to relative Y percentage.
pub fn pixels_to_relative_y(pixels: f64, screen_height: u32) -> f64 {
    pixels / f64::from(screen_height) * 100.0
}

/// Clamp all zones in a layout so they remain within the visible screen
/// bounds. Per-zone X/Y are clamped to `[0, 100 - expanded_size]` so the zone
/// plus its expanded dimensions fit on-screen; expanded sizes are clamped to
/// `[5.0, 100.0]` to keep zones interactable.
pub fn clamp_zones_to_screen(layout: &mut LayoutData) {
    for zone in &mut layout.zones {
        clamp_zone(zone);
    }
}

/// Clamp a single zone's position to valid screen bounds.
pub fn clamp_zone(zone: &mut BentoZone) {
    let max_x = (100.0 - zone.expanded_size.w_percent).max(0.0);
    let max_y = (100.0 - zone.expanded_size.h_percent).max(0.0);

    zone.position.x_percent = zone.position.x_percent.clamp(0.0, max_x);
    zone.position.y_percent = zone.position.y_percent.clamp(0.0, max_y);

    zone.expanded_size.w_percent = zone.expanded_size.w_percent.clamp(5.0, 100.0);
    zone.expanded_size.h_percent = zone.expanded_size.h_percent.clamp(5.0, 100.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{BentoZone, LayoutData, RelativePosition, RelativeSize};
    use smol_str::SmolStr;

    fn make_zone(x: f64, y: f64, w: f64, h: f64) -> BentoZone {
        BentoZone {
            id: SmolStr::new_static("z1"),
            name: "Test".to_string(),
            icon: SmolStr::new_static("T"),
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
            capsule_size: SmolStr::new_static("medium"),
            capsule_shape: SmolStr::new_static("pill"),
            locked: false,
            visible: true,
            created_at: SmolStr::new_static(""),
            updated_at: SmolStr::new_static(""),
            stack_id: None,
            stack_order: 0,
            alias: None,
            display_mode: None,
            live_folder_path: None,
        }
    }

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
        let pixels = relative_x_to_pixels(x_pct, 1920);
        let back = pixels_to_relative_x(pixels, 1920);
        assert!((back - x_pct).abs() < f64::EPSILON);
    }

    #[test]
    fn pixels_to_relative_y_roundtrip() {
        let y_pct = 62.5;
        let pixels = relative_y_to_pixels(y_pct, 1080);
        let back = pixels_to_relative_y(pixels, 1080);
        assert!((back - y_pct).abs() < f64::EPSILON);
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
        assert!((zone.position.x_percent - 70.0).abs() < f64::EPSILON);
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
            version: SmolStr::new_static("1.0.0"),
            zones: vec![
                make_zone(110.0, 110.0, 20.0, 20.0),
                make_zone(-5.0, -5.0, 50.0, 50.0),
            ],
            last_modified: SmolStr::new_static(""),
            coherence_id: None,
        };
        clamp_zones_to_screen(&mut layout);
        assert!((layout.zones[0].position.x_percent - 80.0).abs() < f64::EPSILON);
        assert!((layout.zones[0].position.y_percent - 80.0).abs() < f64::EPSILON);
        assert!((layout.zones[1].position.x_percent - 0.0).abs() < f64::EPSILON);
        assert!((layout.zones[1].position.y_percent - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn current_resolution_returns_nonzero_dimensions() {
        let res = get_current_resolution();
        assert!(res.width > 0, "got {res:?}");
        assert!(res.height > 0, "got {res:?}");
    }

    #[test]
    fn dpi_scale_is_positive() {
        let scale = get_dpi_scale();
        assert!(scale > 0.0, "got {scale}");
    }
}
