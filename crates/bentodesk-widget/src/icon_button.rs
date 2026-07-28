//! `IconButton` — SVG icon with hover animation, dispatching a numeric event
//! id on click.
//!
//! Spec §9: click semantics route through the synchronous dispatcher — we
//! emit a `u32` event id (think Win32 `WM_COMMAND` `wParam`), **not** a
//! `Box<dyn Fn>` callback. Subscribers live in user code and consume the id
//! out of the dispatcher queue.
//!
//! Spec §10: `AnimatedValue<f32>` is `Copy`, sized; no per-frame heap.
//! Hover progress is 0.0 = idle, 1.0 = fully hovered; the renderer
//! interpolates the background alpha from this single channel.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing};

/// Material-design-aligned hover transition (150 ms, ease-out). Exposed as a
/// constant so tests and callers can pin the contract.
pub const HOVER_DURATION_SECS: f32 = 0.150;

/// SVG-icon button. The path string is `&'static str` — icons live in the
/// binary as compile-time literals (spec §10: no runtime parsing of icon
/// metadata in the hot path).
#[derive(Debug, Clone, Copy)]
pub struct IconButton {
    pub svg_path: &'static str,
    pub size: f32,
    /// App-defined event id pushed to the dispatcher on click. Use 0 to mean
    /// "no event" — the dispatcher will ignore zero ids.
    pub on_click_event: u32,
    /// Hover progress 0..=1. Drive via [`IconButton::set_hovered`] +
    /// [`IconButton::tick`] from the frame loop.
    pub hover_anim: AnimatedValue<f32>,
    pub tint: Color,
    pub hover_background: Color,
    pub hover_radius: BorderRadius,
}

impl IconButton {
    /// Construct with the given SVG path data + click event id. Size defaults
    /// to 24px (the DXGI icon convention).
    pub fn new(svg_path: &'static str, on_click_event: u32) -> Self {
        let palette = theme::current().palette;
        Self {
            svg_path,
            size: 24.0,
            on_click_event,
            hover_anim: AnimatedValue::new(0.0),
            tint: palette.text,
            hover_background: palette.hover_overlay,
            hover_radius: theme::radius::DEFAULT.sm,
        }
    }

    /// Begin (or reverse) the hover transition. `hovered=true` animates to
    /// 1.0, `false` animates back to 0.0. Re-entrant calls during an
    /// in-flight transition retarget cleanly thanks to `AnimatedValue`'s
    /// from-current sampling.
    pub fn set_hovered(&mut self, hovered: bool) {
        let target = if hovered { 1.0 } else { 0.0 };
        self.hover_anim
            .animate_to(target, HOVER_DURATION_SECS, Easing::EaseOut);
    }

    /// Advance the hover tween by `dt` seconds. Returns `true` while the
    /// tween is still in flight — the renderer uses this to schedule the
    /// next frame.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.hover_anim.tick(dt)
    }

    /// Sample the current hover progress.
    pub fn hover_progress(&self) -> f32 {
        self.hover_anim.current()
    }

    /// Push the click event id onto the dispatcher. Zero ids are dropped —
    /// callers signal "no event" with `on_click_event = 0`. Returns `true`
    /// when an event was actually pushed.
    pub fn click<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_click_event == 0 {
            return false;
        }
        sink(self.on_click_event);
        true
    }
}

impl LayoutSource for IconButton {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.size),
            height: Length::Px(self.size),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME_PATH: &str = "M12 3L4 9v12h16V9z";

    #[test]
    fn icon_button_hover_anim_progresses_to_one() {
        let mut b = IconButton::new(HOME_PATH, 42);
        assert!((b.hover_progress() - 0.0).abs() < 1e-6);
        b.set_hovered(true);
        // Tick past the duration to guarantee endpoint.
        let active = b.tick(HOVER_DURATION_SECS + 0.01);
        assert!(!active, "tween should complete after duration");
        assert!((b.hover_progress() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn icon_button_hover_anim_reverses_on_unhover() {
        let mut b = IconButton::new(HOME_PATH, 42);
        b.set_hovered(true);
        let _ = b.tick(HOVER_DURATION_SECS + 0.01);
        assert!((b.hover_progress() - 1.0).abs() < 1e-3);
        b.set_hovered(false);
        let _ = b.tick(HOVER_DURATION_SECS + 0.01);
        assert!((b.hover_progress() - 0.0).abs() < 1e-3);
    }

    #[test]
    fn icon_button_click_emits_event_id() {
        let b = IconButton::new(HOME_PATH, 7);
        let mut captured: u32 = 0;
        let pushed = b.click(|id| captured = id);
        assert!(pushed);
        assert_eq!(captured, 7);
    }

    #[test]
    fn icon_button_click_drops_zero_event_id() {
        let b = IconButton::new(HOME_PATH, 0);
        let mut captured: u32 = 0;
        let pushed = b.click(|id| captured = id);
        assert!(!pushed);
        assert_eq!(captured, 0);
    }

    #[test]
    fn icon_button_default_hover_radius_uses_theme_token() {
        let b = IconButton::new(HOME_PATH, 0);
        assert_eq!(b.hover_radius, theme::radius::DEFAULT.sm);
    }
}
