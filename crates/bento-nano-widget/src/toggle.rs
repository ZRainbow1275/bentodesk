//! `Toggle` — on/off switch (iOS-style sliding thumb).
//!
//! Spec §10: `AnimatedValue<f32>` (Copy) drives the 0..1 thumb-position
//! animation; the renderer reads `current()` per frame and lerps both the
//! thumb x-offset (left ↔ right) and the track background between
//! `palette.surface_alt` (off) and `palette.accent` (on).
//!
//! Spec §9: state changes emit a `u32` event id to the dispatcher. Caller
//! interprets the id (e.g. `STEALTH_TOGGLED`) and updates persisted settings.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_tree::{AnimatedValue, Easing};

/// 180 ms thumb travel — slightly longer than checkbox to make the slide
/// motion legible.
pub const TOGGLE_DURATION_SECS: f32 = 0.180;

/// Track 32x18 (≈ React `w-8 h-5` with rounded ends).
pub const TRACK_WIDTH_PX: f32 = 32.0;
pub const TRACK_HEIGHT_PX: f32 = 18.0;
pub const THUMB_DIAMETER_PX: f32 = 14.0;
pub const THUMB_INSET_PX: f32 = (TRACK_HEIGHT_PX - THUMB_DIAMETER_PX) * 0.5;

#[derive(Debug, Clone, Copy)]
pub struct Toggle {
    pub on: bool,
    pub disabled: bool,
    /// 0.0 = off, 1.0 = on. Renderer lerps thumb x and track background.
    pub thumb_anim: AnimatedValue<f32>,
    pub on_change_event: u32,
    pub track_off: Color,
    pub track_on: Color,
    pub thumb: Color,
    pub track_radius: BorderRadius,
    pub thumb_radius: BorderRadius,
}

impl Toggle {
    pub fn new(on_change_event: u32) -> Self {
        let p = theme::current().palette;
        Self {
            on: false,
            disabled: false,
            thumb_anim: AnimatedValue::new(0.0),
            on_change_event,
            track_off: p.surface_alt,
            track_on: p.accent,
            thumb: Color::WHITE,
            track_radius: BorderRadius::all(TRACK_HEIGHT_PX * 0.5),
            thumb_radius: BorderRadius::all(THUMB_DIAMETER_PX * 0.5),
        }
    }

    /// Flip the switch and start the thumb tween. No-op when `disabled`.
    /// Returns the new `on` state.
    pub fn toggle(&mut self) -> bool {
        if self.disabled {
            return self.on;
        }
        self.on = !self.on;
        let target = if self.on { 1.0 } else { 0.0 };
        self.thumb_anim
            .animate_to(target, TOGGLE_DURATION_SECS, Easing::EaseOut);
        self.on
    }

    /// Direct-set the on state (e.g. from a settings restore). Animates if
    /// the target differs; idempotent otherwise.
    pub fn set_on(&mut self, on: bool) {
        if self.on == on {
            return;
        }
        self.on = on;
        let target = if on { 1.0 } else { 0.0 };
        self.thumb_anim
            .animate_to(target, TOGGLE_DURATION_SECS, Easing::EaseOut);
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.thumb_anim.tick(dt)
    }

    pub fn thumb_progress(&self) -> f32 {
        self.thumb_anim.current()
    }

    /// Compute the thumb's left-edge x-offset (DIPs) inside the track at the
    /// current animation progress. Off = `THUMB_INSET_PX`; on = track_width
    /// minus thumb_diameter minus inset.
    pub fn thumb_x(&self) -> f32 {
        let off_x = THUMB_INSET_PX;
        let on_x = TRACK_WIDTH_PX - THUMB_DIAMETER_PX - THUMB_INSET_PX;
        off_x + (on_x - off_x) * self.thumb_progress()
    }

    pub fn emit<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_change_event == 0 {
            return false;
        }
        sink(self.on_change_event);
        true
    }
}

impl LayoutSource for Toggle {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(TRACK_WIDTH_PX),
            height: Length::Px(TRACK_HEIGHT_PX),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_starts_off_with_thumb_at_left() {
        let t = Toggle::new(1);
        assert!(!t.on);
        assert!((t.thumb_x() - THUMB_INSET_PX).abs() < 1e-3);
    }

    #[test]
    fn toggle_flips_and_thumb_moves_to_right() {
        let mut t = Toggle::new(1);
        let after = t.toggle();
        assert!(after);
        let _ = t.tick(TOGGLE_DURATION_SECS + 0.01);
        let expected = TRACK_WIDTH_PX - THUMB_DIAMETER_PX - THUMB_INSET_PX;
        assert!((t.thumb_x() - expected).abs() < 1e-3);
    }

    #[test]
    fn toggle_disabled_is_noop() {
        let mut t = Toggle::new(1);
        t.disabled = true;
        let after = t.toggle();
        assert!(!after);
    }

    #[test]
    fn toggle_emit_pushes_nonzero_event() {
        let t = Toggle::new(99);
        let mut got = 0u32;
        let pushed = t.emit(|id| got = id);
        assert!(pushed);
        assert_eq!(got, 99);
    }
}
