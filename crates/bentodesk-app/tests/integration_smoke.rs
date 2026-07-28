//! Workspace-level integration smoke test.
//!
//! Verifies that `bentodesk-tree` + `bentodesk-layout` + `bentodesk-widget`
//! agree on types and produce a valid layout for a small but realistic tree.
//! The platform / D2D layer is NOT exercised here — that requires a live
//! Direct3D device which CI cannot guarantee.

use bentodesk_app::{AppState, WindowState};
use bentodesk_layout::Direction;
use bentodesk_style::{BorderRadius, Color, Edges, Length, Size};
use bentodesk_widget::{ContainerNode, TextNode, WidgetNode};

#[test]
fn smoke_tree_and_layout_compose() {
    let mut app = AppState::new();
    app.viewport = Size {
        width: 800.0,
        height: 600.0,
    };

    let root = app.mount_root(WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Auto,
        height: Length::Auto,
        padding: Edges::all(24.0),
        background: Color::TRANSPARENT,
        radius: BorderRadius::ZERO,
        shadow: bentodesk_style::Shadow::NONE,
    }));

    let card = WidgetNode::Container(ContainerNode {
        direction: Direction::Column,
        width: Length::Px(600.0),
        height: Length::Px(400.0),
        padding: Edges::all(24.0),
        background: Color::rgba(1.0, 1.0, 1.0, 0.30),
        radius: BorderRadius::all(12.0),
        shadow: bentodesk_style::Shadow::NONE,
    });
    let card_id = app
        .add_child(root, "card", card)
        .ok()
        .unwrap_or(bentodesk_tree::NodeId::ROOT_INVALID);
    assert!(!card_id.is_invalid());

    let label = WidgetNode::Text(TextNode {
        content: std::borrow::Cow::Borrowed("hello"),
        id: None,
        font_size_pt: 16.0,
        font_weight: 400,
        line_height: 1.4,
        color: Color::BLACK,
        width: Length::Auto,
        height: Length::Auto,
    });
    let _ = app.add_child(card_id, "label", label);

    let mut win = WindowState::new();
    let res = win.run_layout(&app);
    assert!(res.is_ok(), "layout must succeed; got {:?}", res.err());

    let result = win.layout.layout(&app.tree, app.viewport);
    assert!(result.is_ok());
    let r = match result {
        Ok(r) => r,
        Err(_) => return,
    };
    // Root + card + label = 3 rectangles.
    assert_eq!(r.len(), 3);
}
