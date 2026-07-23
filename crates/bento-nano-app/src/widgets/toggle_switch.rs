//! iOS-style rocker toggle switch geometry (Wave K1).
//!
//! Used by the Settings panel to render boolean rows (stealth storage,
//! updater auto-download, etc.). The widget is intentionally tiny so it can
//! be inlined into the renderer's hot path without crossing module borders.
//!
//! Visual reference: Tauri `SettingsPanel.css` — 44 x 24 DIP track with a
//! 20 x 20 DIP knob. The off-state knob hugs the left edge of the track,
//! the on-state knob hugs the right edge; the colours come from the active
//! palette (`accent_blue` when on, `surface_subtle` when off — both already
//! defined in `bento_nano_style::tokens`).

use bento_nano_style::Rect;

use crate::settings_panel::{
    SETTINGS_TOGGLE_KNOB_D, SETTINGS_TOGGLE_TRACK_H, SETTINGS_TOGGLE_TRACK_W,
};

/// 2D point in DIPs — local to this widget to avoid pulling `bento-nano-style`'s
/// `Size` into a leaf widget.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Geometry for a single rocker switch.
///
/// `track` is the rounded pill. `knob_off` is the knob position when the
/// switch is off; `knob_on` is the position when on. Renderer picks one
/// based on the current boolean state — there is no animation state here
/// (per spec §10 hot-path discipline; tween is a follow-up wave).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToggleSwitch {
    pub track: Rect,
    pub knob_off: Rect,
    pub knob_on: Rect,
}

impl ToggleSwitch {
    /// Knob position for the supplied boolean state.
    pub fn knob(&self, on: bool) -> Rect {
        if on { self.knob_on } else { self.knob_off }
    }

    /// Track radius (half the track height) — used by the renderer to draw a
    /// fully-rounded pill regardless of how aspect ratio evolves.
    pub fn track_radius(&self) -> f32 {
        self.track.height * 0.5
    }

    /// Knob radius — knobs are circular.
    pub fn knob_radius(&self) -> f32 {
        self.knob_off.height * 0.5
    }
}

/// Compute the geometry for a rocker switch whose top-left corner sits at
/// `origin`. Sizes are pulled from `SETTINGS_TOGGLE_*` constants so the
/// painter, layout, and hit-tester remain byte-stable.
pub fn toggle_switch_layout(origin: Point) -> ToggleSwitch {
    let track = Rect {
        x: origin.x,
        y: origin.y,
        width: SETTINGS_TOGGLE_TRACK_W,
        height: SETTINGS_TOGGLE_TRACK_H,
    };
    // Knob padding — keep the knob 3 DIPs inside the track on all sides.
    let pad = (SETTINGS_TOGGLE_TRACK_H - SETTINGS_TOGGLE_KNOB_D) * 0.5;
    let knob_off = Rect {
        x: track.x + pad,
        y: track.y + pad,
        width: SETTINGS_TOGGLE_KNOB_D,
        height: SETTINGS_TOGGLE_KNOB_D,
    };
    let knob_on = Rect {
        x: track.x + track.width - pad - SETTINGS_TOGGLE_KNOB_D,
        y: knob_off.y,
        width: SETTINGS_TOGGLE_KNOB_D,
        height: SETTINGS_TOGGLE_KNOB_D,
    };
    ToggleSwitch {
        track,
        knob_off,
        knob_on,
    }
}

/// Compute the geometry centred inside an existing rect (e.g. the row's
/// right-anchored control hit-box).
pub fn toggle_switch_in_rect(container: Rect) -> ToggleSwitch {
    let origin = Point {
        x: container.x + (container.width - SETTINGS_TOGGLE_TRACK_W) * 0.5,
        y: container.y + (container.height - SETTINGS_TOGGLE_TRACK_H) * 0.5,
    };
    toggle_switch_layout(origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_track_matches_constants() {
        let s = toggle_switch_layout(Point { x: 100.0, y: 200.0 });
        assert_eq!(s.track.x, 100.0);
        assert_eq!(s.track.y, 200.0);
        assert_eq!(s.track.width, SETTINGS_TOGGLE_TRACK_W);
        assert_eq!(s.track.height, SETTINGS_TOGGLE_TRACK_H);
    }

    #[test]
    fn layout_matches_tauri_settings_source_switch_size() {
        assert_eq!(SETTINGS_TOGGLE_TRACK_W, 44.0);
        assert_eq!(SETTINGS_TOGGLE_TRACK_H, 24.0);
        assert_eq!(SETTINGS_TOGGLE_KNOB_D, 20.0);
    }

    #[test]
    fn knob_off_hugs_track_left() {
        let s = toggle_switch_layout(Point { x: 0.0, y: 0.0 });
        let pad = (SETTINGS_TOGGLE_TRACK_H - SETTINGS_TOGGLE_KNOB_D) * 0.5;
        assert!((s.knob_off.x - pad).abs() < 0.01);
        assert!((s.knob_off.y - pad).abs() < 0.01);
        assert_eq!(s.knob_off.width, SETTINGS_TOGGLE_KNOB_D);
        assert_eq!(s.knob_off.height, SETTINGS_TOGGLE_KNOB_D);
    }

    #[test]
    fn knob_on_hugs_track_right() {
        let s = toggle_switch_layout(Point { x: 0.0, y: 0.0 });
        let pad = (SETTINGS_TOGGLE_TRACK_H - SETTINGS_TOGGLE_KNOB_D) * 0.5;
        let expected_x = SETTINGS_TOGGLE_TRACK_W - pad - SETTINGS_TOGGLE_KNOB_D;
        assert!((s.knob_on.x - expected_x).abs() < 0.01);
        assert_eq!(s.knob_on.y, s.knob_off.y);
    }

    #[test]
    fn knob_off_and_on_differ_along_x_only() {
        let s = toggle_switch_layout(Point { x: 10.0, y: 20.0 });
        assert_ne!(s.knob_off.x, s.knob_on.x);
        assert_eq!(s.knob_off.y, s.knob_on.y);
        assert_eq!(s.knob_off.width, s.knob_on.width);
        assert_eq!(s.knob_off.height, s.knob_on.height);
    }

    #[test]
    fn knob_selector_picks_correct_side() {
        let s = toggle_switch_layout(Point { x: 0.0, y: 0.0 });
        assert_eq!(s.knob(false), s.knob_off);
        assert_eq!(s.knob(true), s.knob_on);
    }

    #[test]
    fn track_radius_is_half_height() {
        let s = toggle_switch_layout(Point { x: 0.0, y: 0.0 });
        assert!((s.track_radius() - SETTINGS_TOGGLE_TRACK_H * 0.5).abs() < 0.01);
    }

    #[test]
    fn knob_radius_is_half_diameter() {
        let s = toggle_switch_layout(Point { x: 0.0, y: 0.0 });
        assert!((s.knob_radius() - SETTINGS_TOGGLE_KNOB_D * 0.5).abs() < 0.01);
    }

    #[test]
    fn centered_in_rect_places_track_at_container_centre() {
        let container = Rect {
            x: 100.0,
            y: 200.0,
            width: 60.0,
            height: 28.0,
        };
        let s = toggle_switch_in_rect(container);
        let expected_x = container.x + (container.width - SETTINGS_TOGGLE_TRACK_W) * 0.5;
        let expected_y = container.y + (container.height - SETTINGS_TOGGLE_TRACK_H) * 0.5;
        assert!((s.track.x - expected_x).abs() < 0.01);
        assert!((s.track.y - expected_y).abs() < 0.01);
    }
}
