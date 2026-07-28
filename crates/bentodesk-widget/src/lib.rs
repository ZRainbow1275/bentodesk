#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bentodesk-widget` — built-in widget set.
//!
//! Spec §3.2: 100% self-rolled controls.
//! Spec §10: every payload field is `Copy` or a hot-path-friendly small type
//! (`SmolStr`, `Arc<Path>`, `Color`). No `String`, no boxed trait objects.
//!
//! The widget enum lives here so the tree (`Tree<WidgetNode>`) and the layout
//! engine (`LayoutSource for WidgetNode`) share one closed type set; adding a
//! new widget is one variant + one match arm rather than a new dyn dispatch.

#![forbid(unsafe_op_in_unsafe_fn)]

// Phase 1 widgets (closed set).
pub mod bento_card;
pub mod button;
pub mod container;
pub mod icon_button;
pub mod image;
pub mod scroll_container;
pub mod text;
pub mod toolbar;

// Phase 2 widgets — inputs.
pub mod checkbox;
pub mod dropdown;
pub mod input;
pub mod radio;
pub mod slider;
pub mod toggle;

// Phase 2 widgets — containers / overlays.
pub mod collapsible;
pub mod modal;
pub mod popup;
pub mod tab;
pub mod tooltip;

// Phase 2 widgets — lists.
pub mod grid;
pub mod list;
pub mod virtual_grid;
pub mod virtual_list;

// Phase 2 widgets — overlay actions.
pub mod context_menu;
pub mod drag_preview;

// Phase 2 widgets — icons.
pub mod file_icon;
pub mod svg_icon;

// Phase 2 widgets — layout primitives.
pub mod column;
pub mod grid_layout;
pub mod row;

pub use bento_card::BentoCard;
pub use button::ButtonNode;
pub use checkbox::Checkbox;
pub use collapsible::Collapsible;
pub use column::Column;
pub use container::ContainerNode;
pub use context_menu::{ContextMenu, ContextMenuItem};
pub use drag_preview::DragPreview;
pub use dropdown::{Dropdown, DropdownOption};
pub use file_icon::FileIcon;
pub use grid::Grid;
pub use grid_layout::GridLayout;
pub use icon_button::{HOVER_DURATION_SECS, IconButton};
pub use image::{ImageNode, ImageSource};
pub use input::Input;
pub use list::List;
pub use modal::{Modal, ModalDismiss};
pub use popup::{Popup, PopupAnchor, PopupPlacement};
pub use radio::Radio;
pub use row::Row;
pub use scroll_container::ScrollContainer;
pub use slider::Slider;
pub use svg_icon::{SvgIcon, SvgSource};
pub use tab::{Tab, TabItem};
pub use text::TextNode;
pub use toggle::Toggle;
pub use toolbar::Toolbar;
pub use tooltip::Tooltip;
pub use virtual_grid::VirtualGrid;
pub use virtual_list::VirtualList;

use bentodesk_layout::{LayoutDesc, LayoutSource};

/// Closed widget enum dispatched at the tree level. New widgets append a
/// variant here and route through the same render / layout match arms in
/// `bentodesk-app`.
#[derive(Debug)]
pub enum WidgetNode {
    Container(ContainerNode),
    Text(TextNode),
    Image(ImageNode),
    Button(ButtonNode),
    BentoCard(BentoCard),
    Toolbar(Toolbar),
    IconButton(IconButton),
    ScrollContainer(ScrollContainer),
    // Phase 2 — inputs.
    Checkbox(Checkbox),
    Toggle(Toggle),
    Radio(Radio),
    Slider(Slider),
    Input(Input),
    Dropdown(Dropdown),
    // Phase 2 — containers / overlays.
    Tab(Tab),
    Collapsible(Collapsible),
    Modal(Modal),
    Popup(Popup),
    Tooltip(Tooltip),
    // Phase 2 — lists.
    List(List),
    Grid(Grid),
    VirtualList(VirtualList),
    VirtualGrid(VirtualGrid),
    // Phase 2 — overlays.
    ContextMenu(ContextMenu),
    DragPreview(DragPreview),
    // Phase 2 — icons.
    SvgIcon(SvgIcon),
    FileIcon(FileIcon),
    // Phase 2 — layout primitives.
    Row(Row),
    Column(Column),
    GridLayout(GridLayout),
}

impl LayoutSource for WidgetNode {
    fn layout(&self) -> LayoutDesc {
        match self {
            WidgetNode::Container(n) => n.layout(),
            WidgetNode::Text(n) => n.layout(),
            WidgetNode::Image(n) => n.layout(),
            WidgetNode::Button(n) => n.layout(),
            WidgetNode::BentoCard(n) => n.layout(),
            WidgetNode::Toolbar(n) => n.layout(),
            WidgetNode::IconButton(n) => n.layout(),
            WidgetNode::ScrollContainer(n) => n.layout(),
            WidgetNode::Checkbox(n) => n.layout(),
            WidgetNode::Toggle(n) => n.layout(),
            WidgetNode::Radio(n) => n.layout(),
            WidgetNode::Slider(n) => n.layout(),
            WidgetNode::Input(n) => n.layout(),
            WidgetNode::Dropdown(n) => n.layout(),
            WidgetNode::Tab(n) => n.layout(),
            WidgetNode::Collapsible(n) => n.layout(),
            WidgetNode::Modal(n) => n.layout(),
            WidgetNode::Popup(n) => n.layout(),
            WidgetNode::Tooltip(n) => n.layout(),
            WidgetNode::List(n) => n.layout(),
            WidgetNode::Grid(n) => n.layout(),
            WidgetNode::VirtualList(n) => n.layout(),
            WidgetNode::VirtualGrid(n) => n.layout(),
            WidgetNode::ContextMenu(n) => n.layout(),
            WidgetNode::DragPreview(n) => n.layout(),
            WidgetNode::SvgIcon(n) => n.layout(),
            WidgetNode::FileIcon(n) => n.layout(),
            WidgetNode::Row(n) => n.layout(),
            WidgetNode::Column(n) => n.layout(),
            WidgetNode::GridLayout(n) => n.layout(),
        }
    }
}

impl WidgetNode {
    /// Stable widget kind tag, used by the renderer to short-circuit dispatch
    /// without a `match` over all enum variants.
    pub fn kind(&self) -> WidgetKind {
        match self {
            WidgetNode::Container(_) => WidgetKind::Container,
            WidgetNode::Text(_) => WidgetKind::Text,
            WidgetNode::Image(_) => WidgetKind::Image,
            WidgetNode::Button(_) => WidgetKind::Button,
            WidgetNode::BentoCard(_) => WidgetKind::BentoCard,
            WidgetNode::Toolbar(_) => WidgetKind::Toolbar,
            WidgetNode::IconButton(_) => WidgetKind::IconButton,
            WidgetNode::ScrollContainer(_) => WidgetKind::ScrollContainer,
            WidgetNode::Checkbox(_) => WidgetKind::Checkbox,
            WidgetNode::Toggle(_) => WidgetKind::Toggle,
            WidgetNode::Radio(_) => WidgetKind::Radio,
            WidgetNode::Slider(_) => WidgetKind::Slider,
            WidgetNode::Input(_) => WidgetKind::Input,
            WidgetNode::Dropdown(_) => WidgetKind::Dropdown,
            WidgetNode::Tab(_) => WidgetKind::Tab,
            WidgetNode::Collapsible(_) => WidgetKind::Collapsible,
            WidgetNode::Modal(_) => WidgetKind::Modal,
            WidgetNode::Popup(_) => WidgetKind::Popup,
            WidgetNode::Tooltip(_) => WidgetKind::Tooltip,
            WidgetNode::List(_) => WidgetKind::List,
            WidgetNode::Grid(_) => WidgetKind::Grid,
            WidgetNode::VirtualList(_) => WidgetKind::VirtualList,
            WidgetNode::VirtualGrid(_) => WidgetKind::VirtualGrid,
            WidgetNode::ContextMenu(_) => WidgetKind::ContextMenu,
            WidgetNode::DragPreview(_) => WidgetKind::DragPreview,
            WidgetNode::SvgIcon(_) => WidgetKind::SvgIcon,
            WidgetNode::FileIcon(_) => WidgetKind::FileIcon,
            WidgetNode::Row(_) => WidgetKind::Row,
            WidgetNode::Column(_) => WidgetKind::Column,
            WidgetNode::GridLayout(_) => WidgetKind::GridLayout,
        }
    }
}

/// Discriminant tag for `WidgetNode`. Keep variants in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Container,
    Text,
    Image,
    Button,
    BentoCard,
    Toolbar,
    IconButton,
    ScrollContainer,
    Checkbox,
    Toggle,
    Radio,
    Slider,
    Input,
    Dropdown,
    Tab,
    Collapsible,
    Modal,
    Popup,
    Tooltip,
    List,
    Grid,
    VirtualList,
    VirtualGrid,
    ContextMenu,
    DragPreview,
    SvgIcon,
    FileIcon,
    Row,
    Column,
    GridLayout,
}
