//! `BentoCard` — rounded, shadowed surface that anchors the BentoDesk look.
//!
//! Spec §3.2: 100% self-rolled. The card owns geometry (radius, padding) and
//! visual (shadow, background) but **not** its child handle. The tree owns
//! parent/child relationships; storing a `child: Option<NodeId>` here would
//! duplicate that wiring and risk drift on detach. We expose a single optional
//! handle for callers who want a shorthand cache, but layout & rendering
//! traverse `tree.children(id)` like every other container.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::tokens as style_tokens;
use bentodesk_style::{BorderRadius, Color, Edges, Length, Shadow};
use bentodesk_theme as theme;
use bentodesk_tree::NodeId;

/// Rounded card with shadow + padded content slot. Default chrome matches the
/// BentoDesk visual language (8px radius, 12px blur, +2px y-offset).
#[derive(Debug, Clone, Copy)]
pub struct BentoCard {
    pub border_radius: BorderRadius,
    pub shadow: Shadow,
    pub background: Color,
    pub padding: Edges,
    pub width: Length,
    pub height: Length,
    /// Optional shortcut to the single content node. `None` is fine — the
    /// renderer always walks `tree.children(id)`. Caching is purely a
    /// convenience for builder-style construction.
    pub child: Option<NodeId>,
}

impl BentoCard {
    /// BentoDesk default chrome: 8px radius, 12px blur, +(0,2) shadow, 12px
    /// inset padding, semi-opaque dark surface. Matches the React design
    /// tokens 1:1 so the spike's visuals stay reference-comparable.
    pub fn default_chrome() -> Self {
        let palette = theme::current().palette;
        Self {
            border_radius: BorderRadius::all(8.0),
            // Wave B migration: pulled from `bentodesk_style::tokens::SHADOW.ink_card`
            // (offset (0,2), blur 12, color 0x00000040). Byte-identical to the
            // pre-migration literal — test in `tokens::tests` guards parity.
            shadow: style_tokens::SHADOW.ink_card,
            background: palette.surface,
            padding: Edges::all(12.0),
            width: Length::Auto,
            height: Length::Auto,
            child: None,
        }
    }

    /// Attach the cached child handle. Caller must still call
    /// `tree.append_child(card_node, child)` — this only updates the
    /// shorthand pointer.
    pub fn with_child(mut self, child: NodeId) -> Self {
        self.child = Some(child);
        self
    }
}

impl Default for BentoCard {
    fn default() -> Self {
        Self::default_chrome()
    }
}

impl LayoutSource for BentoCard {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            // Single-child container — direction doesn't matter visually,
            // but Column matches our typical vertical content.
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bento_card_default_chrome_has_8px_radius() {
        let c = BentoCard::default_chrome();
        assert_eq!(c.border_radius.top_left, 8.0);
        assert_eq!(c.border_radius.top_right, 8.0);
        assert_eq!(c.border_radius.bottom_left, 8.0);
        assert_eq!(c.border_radius.bottom_right, 8.0);
        // Shadow defaults — sanity-pin the BentoDesk tokens.
        assert_eq!(c.shadow.offset_y, 2.0);
        assert_eq!(c.shadow.blur, 12.0);
        assert_eq!(c.padding.top, 12.0);
    }

    #[test]
    fn bento_card_with_child_caches_handle() {
        let c = BentoCard::default_chrome();
        assert!(c.child.is_none());
        // Constructing a NodeId directly only via Tree, so use the invalid
        // sentinel for this pure-data check.
        let id = bentodesk_tree::NodeId::ROOT_INVALID;
        let c = c.with_child(id);
        assert_eq!(c.child, Some(id));
    }
}
