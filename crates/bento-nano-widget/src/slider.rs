//! `Slider` — single-thumb horizontal slider with reactive value (`Signal<f32>`)
//! clamped to `[0, 1]`. Visual chrome: track + filled portion left of thumb +
//! circular thumb with hover halo.
//!
//! Spec §10: `Signal<f32>` mirrors the value so observers can mark themselves
//! dirty when the slider moves; equal-value writes are dropped silently so
//! drag events that hit the clamp don't spuriously re-render.
//!
//! Spec §9: drag commit emits a dispatcher `u32` event id with the final
//! value snapshotted via the caller-provided sink.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length};
use bento_nano_theme as theme;
use bento_nano_tree::{AnimatedValue, Easing, Signal};

pub const HOVER_DURATION_SECS: f32 = 0.150;
pub const TRACK_HEIGHT_PX: f32 = 4.0;
pub const THUMB_DIAMETER_PX: f32 = 16.0;
pub const DEFAULT_WIDTH_PX: f32 = 200.0;

#[derive(Debug)]
pub struct Slider {
    /// Reactive value in `[0.0, 1.0]`. Caller may map to a domain range
    /// (e.g. 0..100% volume) on read.
    pub value: Signal<f32>,
    pub width: f32,
    pub disabled: bool,
    /// Hover halo around thumb — 0 = idle, 1 = fully hovered.
    pub hover_anim: AnimatedValue<f32>,
    /// Set true while the user is mid-drag; the renderer can use this to
    /// suppress the hover-out tween until the drag releases.
    pub dragging: bool,
    /// Dispatcher event pushed on drag commit (mouse-up). Zero = drop.
    pub on_commit_event: u32,
    pub track_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
    pub track_radius: BorderRadius,
    pub thumb_radius: BorderRadius,
}

impl Slider {
    pub fn new(initial: f32, on_commit_event: u32) -> Self {
        let p = theme::current().palette;
        let clamped = initial.clamp(0.0, 1.0);
        Self {
            value: Signal::new(clamped),
            width: DEFAULT_WIDTH_PX,
            disabled: false,
            hover_anim: AnimatedValue::new(0.0),
            dragging: false,
            on_commit_event,
            track_color: p.surface_alt,
            fill_color: p.accent,
            thumb_color: Color::WHITE,
            track_radius: BorderRadius::all(TRACK_HEIGHT_PX * 0.5),
            thumb_radius: BorderRadius::all(THUMB_DIAMETER_PX * 0.5),
        }
    }

    /// Compute the thumb's center x-offset (DIPs) at the current value.
    /// `0.0` value → `THUMB_DIAMETER_PX/2`; `1.0` value → `width - THUMB_DIAMETER_PX/2`.
    pub fn thumb_center_x(&self) -> f32 {
        let half_thumb = THUMB_DIAMETER_PX * 0.5;
        let travel = (self.width - THUMB_DIAMETER_PX).max(0.0);
        half_thumb + travel * self.current_value()
    }

    /// Current (clamped) value.
    pub fn current_value(&self) -> f32 {
        *self.value.get()
    }

    /// Set the value programmatically; clamps + drops equal writes.
    /// Returns `true` when the signal actually changed.
    pub fn set_value(&mut self, v: f32) -> bool {
        if self.disabled {
            return false;
        }
        let clamped = v.clamp(0.0, 1.0);
        self.value.set(clamped)
    }

    /// Convert a pointer x in widget-local coordinates to a normalised value
    /// and update the signal. Returns the new value (clamped).
    pub fn drag_to(&mut self, pointer_x: f32) -> f32 {
        if self.disabled {
            return self.current_value();
        }
        let half_thumb = THUMB_DIAMETER_PX * 0.5;
        let travel = (self.width - THUMB_DIAMETER_PX).max(1.0);
        let raw = (pointer_x - half_thumb) / travel;
        let clamped = raw.clamp(0.0, 1.0);
        let _ = self.value.set(clamped);
        clamped
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        if self.disabled {
            return;
        }
        let target = if hovered { 1.0 } else { 0.0 };
        self.hover_anim
            .animate_to(target, HOVER_DURATION_SECS, Easing::EaseOut);
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.hover_anim.tick(dt)
    }

    pub fn hover_progress(&self) -> f32 {
        self.hover_anim.current()
    }

    /// Push the commit event id to the dispatcher sink. Returns `true` when
    /// an event was pushed (zero ids are dropped).
    pub fn emit_commit<F: FnMut(u32, f32)>(&self, mut sink: F) -> bool {
        if self.on_commit_event == 0 {
            return false;
        }
        sink(self.on_commit_event, self.current_value());
        true
    }
}

impl LayoutSource for Slider {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.width),
            height: Length::Px(THUMB_DIAMETER_PX),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_initial_value_is_clamped() {
        let s = Slider::new(2.0, 0);
        assert!((s.current_value() - 1.0).abs() < 1e-6);
        let s = Slider::new(-0.5, 0);
        assert!((s.current_value() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn slider_thumb_x_at_zero_is_half_thumb() {
        let s = Slider::new(0.0, 0);
        assert!((s.thumb_center_x() - THUMB_DIAMETER_PX * 0.5).abs() < 1e-3);
    }

    #[test]
    fn slider_thumb_x_at_one_is_right_edge_minus_half_thumb() {
        let s = Slider::new(1.0, 0);
        let expected = DEFAULT_WIDTH_PX - THUMB_DIAMETER_PX * 0.5;
        assert!((s.thumb_center_x() - expected).abs() < 1e-3);
    }

    #[test]
    fn slider_drag_to_outside_clamps() {
        let mut s = Slider::new(0.5, 0);
        let v = s.drag_to(-100.0);
        assert!((v - 0.0).abs() < 1e-6);
        let v = s.drag_to(99999.0);
        assert!((v - 1.0).abs() < 1e-6);
    }

    #[test]
    fn slider_set_value_dirty_after_change() {
        let mut s = Slider::new(0.5, 0);
        s.value.clear_dirty();
        let changed = s.set_value(0.75);
        assert!(changed);
        assert!(s.value.is_dirty());
    }

    #[test]
    fn slider_set_value_equal_is_dropped() {
        let mut s = Slider::new(0.5, 0);
        s.value.clear_dirty();
        let changed = s.set_value(0.5);
        assert!(!changed);
        assert!(!s.value.is_dirty());
    }

    #[test]
    fn slider_emit_commit_returns_value_to_sink() {
        let s = Slider::new(0.42, 7);
        let mut got_id = 0u32;
        let mut got_v = 0.0f32;
        let pushed = s.emit_commit(|id, v| {
            got_id = id;
            got_v = v;
        });
        assert!(pushed);
        assert_eq!(got_id, 7);
        assert!((got_v - 0.42).abs() < 1e-6);
    }
}
