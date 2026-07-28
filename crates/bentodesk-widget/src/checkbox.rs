//! `Checkbox` — binary on/off control with check-mark fill animation.
//!
//! Spec §10: `AnimatedValue<f32>` (Copy) drives the 0..1 fill progress; the
//! renderer reads `current()` per frame and lerps the check-mark stroke alpha
//! plus the box background between `palette.surface_alt` and `palette.accent`.
//!
//! Spec §9: click semantics route through the synchronous dispatcher — we
//! emit a `u32` event id on toggle, never a `Box<dyn Fn>`. Callers consume
//! the event id from the dispatcher queue and update their domain model.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing};

/// Material-aligned 120 ms check transition.
pub const CHECK_DURATION_SECS: f32 = 0.120;

/// 16x16 default — matches the React design tokens (`w-4 h-4`).
pub const DEFAULT_SIZE_PX: f32 = 16.0;

#[derive(Debug, Clone, Copy)]
pub struct Checkbox {
    pub size: f32,
    /// Cached state — also drives the animation target. Use [`Self::set_checked`]
    /// to mutate so the tween starts in the right direction.
    pub checked: bool,
    pub disabled: bool,
    /// 0.0 = unchecked, 1.0 = checked. Renderer uses this to fade the
    /// check-mark in / out and lerp box background colour.
    pub fill_anim: AnimatedValue<f32>,
    /// Dispatcher event id pushed on toggle. Zero = "no event" (dropped).
    pub on_toggle_event: u32,
    pub box_color: Color,
    pub box_color_checked: Color,
    pub check_color: Color,
    pub border: Color,
    pub radius: BorderRadius,
}

impl Checkbox {
    pub fn new(on_toggle_event: u32) -> Self {
        let p = theme::current().palette;
        Self {
            size: DEFAULT_SIZE_PX,
            checked: false,
            disabled: false,
            fill_anim: AnimatedValue::new(0.0),
            on_toggle_event,
            box_color: p.surface_alt,
            box_color_checked: p.accent,
            check_color: Color::WHITE,
            border: p.border,
            radius: BorderRadius::all(3.0),
        }
    }

    /// Toggle and start the fill tween. No-op when `disabled`. Returns the
    /// new `checked` state.
    pub fn toggle(&mut self) -> bool {
        if self.disabled {
            return self.checked;
        }
        self.checked = !self.checked;
        let target = if self.checked { 1.0 } else { 0.0 };
        self.fill_anim
            .animate_to(target, CHECK_DURATION_SECS, Easing::EaseOut);
        self.checked
    }

    /// Direct-set the checked state (e.g. from external store hydration).
    /// Animates in the appropriate direction; idempotent if already at target.
    pub fn set_checked(&mut self, checked: bool) {
        if self.checked == checked {
            return;
        }
        self.checked = checked;
        let target = if checked { 1.0 } else { 0.0 };
        self.fill_anim
            .animate_to(target, CHECK_DURATION_SECS, Easing::EaseOut);
    }

    /// Advance the fill tween. Returns `true` while in flight so the renderer
    /// can request the next frame.
    pub fn tick(&mut self, dt: f32) -> bool {
        self.fill_anim.tick(dt)
    }

    pub fn fill_progress(&self) -> f32 {
        self.fill_anim.current()
    }

    /// Push the toggle event id to the dispatcher sink. Returns `true` when
    /// an event was actually pushed (zero ids are dropped).
    pub fn emit<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_toggle_event == 0 {
            return false;
        }
        sink(self.on_toggle_event);
        true
    }
}

impl LayoutSource for Checkbox {
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

    #[test]
    fn checkbox_toggle_flips_state_and_starts_anim() {
        let mut c = Checkbox::new(7);
        assert!(!c.checked);
        assert!((c.fill_progress() - 0.0).abs() < 1e-6);
        let after = c.toggle();
        assert!(after);
        assert!(c.checked);
        // Tween in flight; a tick longer than the duration completes it.
        let active = c.tick(CHECK_DURATION_SECS + 0.01);
        assert!(!active);
        assert!((c.fill_progress() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn checkbox_disabled_toggle_is_noop() {
        let mut c = Checkbox::new(7);
        c.disabled = true;
        let after = c.toggle();
        assert!(!after);
        assert!(!c.checked);
    }

    #[test]
    fn checkbox_set_checked_idempotent_skips_anim() {
        let mut c = Checkbox::new(7);
        c.set_checked(false); // already false — no anim.
        let active = c.tick(CHECK_DURATION_SECS + 0.01);
        assert!(!active);
        assert!((c.fill_progress() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn checkbox_emit_drops_zero_event() {
        let c = Checkbox::new(0);
        let mut got = 0u32;
        let pushed = c.emit(|id| got = id);
        assert!(!pushed);
        assert_eq!(got, 0);
    }

    #[test]
    fn checkbox_emit_pushes_nonzero_event() {
        let c = Checkbox::new(42);
        let mut got = 0u32;
        let pushed = c.emit(|id| got = id);
        assert!(pushed);
        assert_eq!(got, 42);
    }
}
