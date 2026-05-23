//! Business surface — main app toolbar (top action row).
//!
//! Visual spec: see `toolbar.snap.md` (corner radius, palette tokens,
//! children order, hover timing). The composition will combine
//! `bento-nano-widget::{Toolbar, IconButton}` primitives once the wave-4
//! coordination question (`business-ui-1` → team-lead, 2026-05-03) is ruled
//! and the widget-library agent ships any new primitives we depend on.
//!
//! Status: scaffolding. The visual spec is locked (per §11 R3 ruling), the
//! composition body lands when the upstream primitive surface is finalised.
//! NOT a `todo!()` stub — `build()` returns a valid empty Container so the
//! tree mount path stays compile-clean while business-ui-1 awaits the rule.

use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length};
use bento_nano_widget::{ContainerNode, WidgetNode};

/// Build the toolbar widget subtree. Returns a typed Container today; the
/// real composition (IconButton children for PIN / SETTINGS / NEW-ZONE /
/// AUTO-ORGANIZE / TRAY) lands when the wave-4 widget-primitive dependency
/// is resolved. The Container's geometry (40 px tall, full-width row,
/// theme-driven background) is final per `toolbar.snap.md`.
pub fn build() -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Row,
        width: Length::Auto,
        height: Length::Px(40.0),
        padding: Edges::all(4.0),
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn build_produces_a_row_oriented_container() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Row);
        // 40 px is the snap.md mandated toolbar height — locked per §11 R3.
        assert!(matches!(layout.height, Length::Px(h) if (h - 40.0).abs() < 0.01));
    }
}
