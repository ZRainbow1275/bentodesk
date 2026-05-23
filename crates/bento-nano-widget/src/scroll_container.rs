//! `ScrollContainer` — vertical scroll viewport with content clip.
//!
//! Spec §10: `Signal<f32>` (Phase 1.1) drives the offset; equal-value writes
//! are dropped silently so wheel events that hit the clamp don't spuriously
//! re-render. The renderer pushes a clip rect derived from the offset down
//! to the child; we deliberately do **not** draw a scrollbar — BentoDesk's
//! visual language is scrollbar-free, gesture-driven.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Edges, Length};
use bento_nano_tree::{NodeId, Signal};

/// Vertical scroll container. Holds its scroll offset reactively so widgets
/// observing the same signal can mark themselves dirty when the offset
/// changes.
#[derive(Debug)]
pub struct ScrollContainer {
    pub scroll_offset: Signal<f32>,
    pub viewport_height: f32,
    pub content_height: f32,
    pub width: Length,
    pub child: Option<NodeId>,
}

impl ScrollContainer {
    pub fn new(viewport_height: f32, content_height: f32) -> Self {
        Self {
            scroll_offset: Signal::new(0.0),
            viewport_height,
            content_height,
            width: Length::Auto,
            child: None,
        }
    }

    /// Maximum legal offset: 0.0 when content fits inside the viewport.
    pub fn max_offset(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Apply a wheel delta (positive = scroll down). Clamps to
    /// `[0, max_offset()]`. Returns the new offset.
    pub fn scroll_by(&mut self, delta: f32) -> f32 {
        let next = (*self.scroll_offset.get() + delta).clamp(0.0, self.max_offset());
        let _ = self.scroll_offset.set(next);
        next
    }

    /// Direct-set the offset, clamped. Returns true when the value actually
    /// changed (forwarded from `Signal::set`).
    pub fn set_offset(&mut self, offset: f32) -> bool {
        let clamped = offset.clamp(0.0, self.max_offset());
        self.scroll_offset.set(clamped)
    }

    /// True when scrolling is meaningful (content overflows viewport).
    pub fn is_scrollable(&self) -> bool {
        self.content_height > self.viewport_height
    }
}

impl LayoutSource for ScrollContainer {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: self.width,
            height: Length::Px(self.viewport_height),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_container_clamps_offset_to_max() {
        let mut s = ScrollContainer::new(100.0, 300.0);
        assert!((s.max_offset() - 200.0).abs() < 1e-6);
        // Scroll past the max — should clamp to 200.
        let n = s.scroll_by(500.0);
        assert!((n - 200.0).abs() < 1e-6);
        // Scroll back past 0 — should clamp to 0.
        let n = s.scroll_by(-1000.0);
        assert!((n - 0.0).abs() < 1e-6);
    }

    #[test]
    fn scroll_container_signal_dirty_after_set() {
        let mut s = ScrollContainer::new(100.0, 300.0);
        // Fresh signal — clean.
        assert!(!s.scroll_offset.is_dirty());
        let changed = s.set_offset(50.0);
        assert!(changed);
        assert!(
            s.scroll_offset.is_dirty(),
            "set must flip the signal dirty flag"
        );
        // Equal-value rewrite must not flip dirty after we cleared it.
        s.scroll_offset.clear_dirty();
        let changed = s.set_offset(50.0);
        assert!(!changed);
        assert!(!s.scroll_offset.is_dirty());
    }

    #[test]
    fn scroll_container_no_scroll_when_content_fits() {
        let s = ScrollContainer::new(300.0, 200.0);
        assert!(!s.is_scrollable());
        assert_eq!(s.max_offset(), 0.0);
    }
}
