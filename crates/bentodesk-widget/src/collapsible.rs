//! `Collapsible` — header + expandable body. Body height is animated from 0
//! → measured-height (and back) on toggle. Caller supplies the natural body
//! height; the widget interpolates a current height the renderer uses for
//! clipping.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use bentodesk_tree::{AnimatedValue, Easing};
use smol_str::SmolStr;

pub const EXPAND_DURATION_SECS: f32 = 0.220;

#[derive(Debug, Clone)]
pub struct Collapsible {
    pub header_label: SmolStr,
    /// Natural body height once expanded (DIPs). Caller measures via the
    /// layout pass on the body subtree before construction.
    pub body_natural_height: f32,
    pub expanded: bool,
    /// 0 = collapsed, 1 = fully expanded.
    pub expand_anim: AnimatedValue<f32>,
    pub on_toggle_event: u32,
    pub header_height: f32,
    pub padding: Edges,
    pub background: Color,
    pub header_color: Color,
    pub border_radius: BorderRadius,
}

impl Collapsible {
    pub fn new(header_label: impl Into<SmolStr>, body_height: f32) -> Self {
        let p = theme::current().palette;
        Self {
            header_label: header_label.into(),
            body_natural_height: body_height,
            expanded: false,
            expand_anim: AnimatedValue::new(0.0),
            on_toggle_event: 0,
            header_height: 32.0,
            padding: Edges::all(8.0),
            background: p.surface,
            header_color: p.text,
            border_radius: BorderRadius::all(6.0),
        }
    }

    pub fn toggle(&mut self) -> bool {
        self.expanded = !self.expanded;
        let target = if self.expanded { 1.0 } else { 0.0 };
        self.expand_anim
            .animate_to(target, EXPAND_DURATION_SECS, Easing::EaseInOut);
        self.expanded
    }

    /// Direct-set the expanded state — animates if the target differs.
    pub fn set_expanded(&mut self, expanded: bool) {
        if self.expanded == expanded {
            return;
        }
        self.expanded = expanded;
        let target = if expanded { 1.0 } else { 0.0 };
        self.expand_anim
            .animate_to(target, EXPAND_DURATION_SECS, Easing::EaseInOut);
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        self.expand_anim.tick(dt)
    }

    pub fn expand_progress(&self) -> f32 {
        self.expand_anim.current()
    }

    /// Current rendered body height. Renderer uses this as a clip rect over
    /// the body subtree so partial-expand frames look like a wipe.
    pub fn current_body_height(&self) -> f32 {
        self.body_natural_height * self.expand_progress()
    }

    /// Total height = header + animated body.
    pub fn total_height(&self) -> f32 {
        self.header_height + self.current_body_height()
    }

    pub fn emit<F: FnMut(u32)>(&self, mut sink: F) -> bool {
        if self.on_toggle_event == 0 {
            return false;
        }
        sink(self.on_toggle_event);
        true
    }
}

impl LayoutSource for Collapsible {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: Length::Auto,
            height: Length::Px(self.total_height()),
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsible_starts_collapsed_with_header_only_height() {
        let c = Collapsible::new("Section", 200.0);
        assert!(!c.expanded);
        assert!((c.current_body_height() - 0.0).abs() < 1e-6);
        assert!((c.total_height() - 32.0).abs() < 1e-6);
    }

    #[test]
    fn collapsible_toggle_expands_to_full_body_after_anim() {
        let mut c = Collapsible::new("Section", 200.0);
        let after = c.toggle();
        assert!(after);
        let _ = c.tick(EXPAND_DURATION_SECS + 0.01);
        assert!((c.current_body_height() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn collapsible_toggle_back_collapses_to_header_only() {
        let mut c = Collapsible::new("Section", 200.0);
        c.set_expanded(true);
        let _ = c.tick(EXPAND_DURATION_SECS + 0.01);
        c.toggle();
        let _ = c.tick(EXPAND_DURATION_SECS + 0.01);
        assert!((c.current_body_height() - 0.0).abs() < 1e-3);
    }
}
