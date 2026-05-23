//! `Tooltip` — delayed-show wrapper around [`crate::popup::Popup`]. Holds an
//! anchor + label + show/hide delays; the runtime ticks it from the frame
//! loop and the renderer reads `is_visible()` to decide whether to draw the
//! popup body.
//!
//! Spec §10: `SmolStr` label inline ≤22 bytes; tooltip text in BentoDesk is
//! short ("Pin", "Edit zone…", "Capture screenshot") so this never spills to
//! heap in practice.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Edges, Length, Rect, Size};
use smol_str::SmolStr;

use crate::popup::{Popup, PopupAnchor, PopupPlacement};

/// Material-aligned 500 ms hover-to-show delay; 100 ms hide grace so moving
/// from one tooltip-bearing surface to a sibling doesn't blink.
pub const SHOW_DELAY_SECS: f32 = 0.500;
pub const HIDE_DELAY_SECS: f32 = 0.100;

#[derive(Debug, Clone)]
pub struct Tooltip {
    pub label: SmolStr,
    pub popup: Popup,
    /// Seconds the hover has been active; tooltip becomes visible at
    /// `>= SHOW_DELAY_SECS`. Reset to 0 when the pointer leaves and resumes
    /// growing on re-entry — caller manages lifecycle by calling
    /// [`Self::pointer_enter`] / [`Self::pointer_leave`].
    hover_elapsed: f32,
    /// Seconds since pointer left while still in the hide-grace window.
    /// Tooltip becomes hidden at `>= HIDE_DELAY_SECS`.
    leave_elapsed: f32,
    state: TooltipState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TooltipState {
    Idle,
    Hovering,
    Showing,
    Leaving,
}

impl Tooltip {
    pub fn new(label: impl Into<SmolStr>) -> Self {
        // Tooltip default size — auto-shrink would need DWrite measurement;
        // 200×32 is the default until the renderer measures the label.
        let popup = Popup::new(Size {
            width: 200.0,
            height: 32.0,
        });
        Self {
            label: label.into(),
            popup,
            hover_elapsed: 0.0,
            leave_elapsed: 0.0,
            state: TooltipState::Idle,
        }
    }

    /// Bind the tooltip to the anchor rect of the hovered widget. Caller
    /// supplies the rect from the layout pass.
    pub fn set_anchor(&mut self, anchor: PopupAnchor, placement: PopupPlacement) {
        self.popup.anchor = anchor;
        self.popup.placement = placement;
    }

    /// Pointer entered the host widget — start (or resume) the show timer.
    pub fn pointer_enter(&mut self) {
        match self.state {
            TooltipState::Idle | TooltipState::Leaving => {
                self.state = TooltipState::Hovering;
                self.hover_elapsed = 0.0;
            }
            TooltipState::Hovering | TooltipState::Showing => {}
        }
        self.leave_elapsed = 0.0;
    }

    /// Pointer left — start the hide-grace timer. If the pointer re-enters
    /// before `HIDE_DELAY_SECS` elapses the tooltip stays visible.
    pub fn pointer_leave(&mut self) {
        match self.state {
            TooltipState::Showing => {
                self.state = TooltipState::Leaving;
                self.leave_elapsed = 0.0;
            }
            TooltipState::Hovering => {
                self.state = TooltipState::Idle;
                self.hover_elapsed = 0.0;
            }
            TooltipState::Idle | TooltipState::Leaving => {}
        }
    }

    /// Advance the timers by `dt` seconds. Returns `true` while at least one
    /// timer is still running so the renderer schedules another frame.
    pub fn tick(&mut self, dt: f32) -> bool {
        match self.state {
            TooltipState::Hovering => {
                self.hover_elapsed += dt;
                if self.hover_elapsed >= SHOW_DELAY_SECS {
                    self.state = TooltipState::Showing;
                    self.popup.show();
                }
                true
            }
            TooltipState::Leaving => {
                self.leave_elapsed += dt;
                if self.leave_elapsed >= HIDE_DELAY_SECS {
                    self.state = TooltipState::Idle;
                    self.popup.hide();
                    self.hover_elapsed = 0.0;
                }
                true
            }
            TooltipState::Idle | TooltipState::Showing => false,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.popup.visible
    }

    /// Resolved screen rect against the available `screen` size — convenience
    /// pass-through to the underlying popup.
    pub fn resolve_rect(&self, screen: Size) -> (Rect, PopupPlacement) {
        self.popup.resolve_rect(screen)
    }
}

impl LayoutSource for Tooltip {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.popup.content_size.width),
            height: Length::Px(self.popup.content_size.height),
            padding: Edges::xy(8.0, 4.0),
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_shows_after_hover_delay() {
        let mut t = Tooltip::new("Pin zone");
        t.pointer_enter();
        assert!(!t.is_visible());
        let still = t.tick(SHOW_DELAY_SECS - 0.1);
        assert!(still);
        assert!(!t.is_visible());
        let _ = t.tick(0.2);
        assert!(t.is_visible());
    }

    #[test]
    fn tooltip_hides_after_leave_grace() {
        let mut t = Tooltip::new("Pin");
        t.pointer_enter();
        let _ = t.tick(SHOW_DELAY_SECS + 0.01);
        assert!(t.is_visible());
        t.pointer_leave();
        let _ = t.tick(HIDE_DELAY_SECS + 0.01);
        assert!(!t.is_visible());
    }

    #[test]
    fn tooltip_re_enter_during_grace_keeps_visible() {
        let mut t = Tooltip::new("Pin");
        t.pointer_enter();
        let _ = t.tick(SHOW_DELAY_SECS + 0.01);
        assert!(t.is_visible());
        t.pointer_leave();
        let _ = t.tick(HIDE_DELAY_SECS * 0.5);
        // Re-enter mid-grace.
        t.pointer_enter();
        let _ = t.tick(HIDE_DELAY_SECS);
        assert!(t.is_visible());
    }

    #[test]
    fn tooltip_leave_during_hover_resets_without_show() {
        let mut t = Tooltip::new("Pin");
        t.pointer_enter();
        let _ = t.tick(SHOW_DELAY_SECS * 0.4);
        assert!(!t.is_visible());
        t.pointer_leave();
        let _ = t.tick(SHOW_DELAY_SECS);
        assert!(!t.is_visible());
    }
}
