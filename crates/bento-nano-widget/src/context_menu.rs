//! `ContextMenu` — right-click cascading menu. Composes [`crate::popup::Popup`]
//! for placement with an inline list of menu items. Cascading sub-menus use a
//! nested ContextMenu opened against the parent item's anchor rect.

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{Color, Edges, Length, Size};
use bento_nano_theme as theme;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::popup::{Popup, PopupAnchor, PopupPlacement};

#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: SmolStr,
    /// Dispatcher event id pushed when this item is activated. Zero = drop.
    pub event_id: u32,
    pub disabled: bool,
    /// `true` renders as a divider; `label` and `event_id` are ignored.
    pub divider: bool,
}

impl ContextMenuItem {
    pub fn new(label: impl Into<SmolStr>, event_id: u32) -> Self {
        Self {
            label: label.into(),
            event_id,
            disabled: false,
            divider: false,
        }
    }

    pub fn divider() -> Self {
        Self {
            label: SmolStr::new_static(""),
            event_id: 0,
            disabled: false,
            divider: true,
        }
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

pub const ITEM_HEIGHT_PX: f32 = 28.0;
pub const DIVIDER_HEIGHT_PX: f32 = 9.0; // 4 + 1 + 4 padding/line
pub const DEFAULT_WIDTH_PX: f32 = 200.0;

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub items: SmallVec<[ContextMenuItem; 12]>,
    pub popup: Popup,
    /// Hover index — `Some(i)` highlights item `i`. Renderer paints
    /// `palette.hover_overlay` behind the row.
    pub hovered: Option<u32>,
    pub item_color: Color,
    pub item_color_disabled: Color,
    pub divider_color: Color,
}

impl ContextMenu {
    pub fn new(items: impl IntoIterator<Item = ContextMenuItem>) -> Self {
        let p = theme::current().palette;
        let items: SmallVec<[ContextMenuItem; 12]> = items.into_iter().collect();
        let height = compute_total_height(&items);
        let popup = Popup::new(Size {
            width: DEFAULT_WIDTH_PX,
            height,
        });
        Self {
            items,
            popup,
            hovered: None,
            item_color: p.text,
            item_color_disabled: p.text_muted,
            divider_color: p.border,
        }
    }

    /// Open at the cursor anchor (typically a 1×1 rect at the click point).
    pub fn open_at(&mut self, anchor: PopupAnchor) {
        self.popup.anchor = anchor;
        self.popup.placement = PopupPlacement::Bottom;
        self.popup.show();
    }

    pub fn close(&mut self) {
        self.popup.hide();
        self.hovered = None;
    }

    pub fn is_open(&self) -> bool {
        self.popup.visible
    }

    /// Move hover to `index` if the item exists and is enabled.
    pub fn set_hover(&mut self, index: Option<u32>) {
        self.hovered = match index {
            None => None,
            Some(i) => {
                let usz = i as usize;
                match self.items.get(usz) {
                    Some(item) if !item.disabled && !item.divider => Some(i),
                    _ => None,
                }
            }
        };
    }

    /// Activate the item at `index` — pushes its event id to `sink`. Closes
    /// the menu on success. Disabled / divider / out-of-range items are
    /// silently dropped.
    pub fn activate<F: FnMut(u32)>(&mut self, index: u32, mut sink: F) -> bool {
        let usz = index as usize;
        let item = match self.items.get(usz) {
            Some(i) => i,
            None => return false,
        };
        if item.disabled || item.divider || item.event_id == 0 {
            return false;
        }
        sink(item.event_id);
        self.close();
        true
    }
}

fn compute_total_height(items: &SmallVec<[ContextMenuItem; 12]>) -> f32 {
    let mut h = 0.0_f32;
    for it in items {
        h += if it.divider {
            DIVIDER_HEIGHT_PX
        } else {
            ITEM_HEIGHT_PX
        };
    }
    // Outer padding 4 + 4.
    h + 8.0
}

impl LayoutSource for ContextMenu {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: Length::Px(self.popup.content_size.width),
            height: Length::Px(self.popup.content_size.height),
            padding: Edges::xy(0.0, 4.0),
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::Rect;

    fn three_items() -> ContextMenu {
        ContextMenu::new([
            ContextMenuItem::new("Pin", 1),
            ContextMenuItem::new("Edit zone…", 2),
            ContextMenuItem::divider(),
            ContextMenuItem::new("Delete", 3).disabled(true),
        ])
    }

    #[test]
    fn context_menu_open_sets_anchor_and_visible() {
        let mut m = three_items();
        let cursor = Rect {
            x: 50.0,
            y: 50.0,
            width: 1.0,
            height: 1.0,
        };
        m.open_at(cursor);
        assert!(m.is_open());
        assert!((m.popup.anchor.x - 50.0).abs() < 1e-3);
    }

    #[test]
    fn context_menu_activate_pushes_event_and_closes() {
        let mut m = three_items();
        m.open_at(Rect::ZERO);
        let mut got = 0u32;
        let ok = m.activate(0, |id| got = id);
        assert!(ok);
        assert_eq!(got, 1);
        assert!(!m.is_open());
    }

    #[test]
    fn context_menu_activate_disabled_drops() {
        let mut m = three_items();
        m.open_at(Rect::ZERO);
        let mut got = 0u32;
        let ok = m.activate(3, |id| got = id);
        assert!(!ok);
        assert_eq!(got, 0);
        // Menu stays open on a no-op activate.
        assert!(m.is_open());
    }

    #[test]
    fn context_menu_activate_divider_drops() {
        let mut m = three_items();
        m.open_at(Rect::ZERO);
        let mut got = 0u32;
        let ok = m.activate(2, |id| got = id);
        assert!(!ok);
        assert_eq!(got, 0);
    }

    #[test]
    fn context_menu_set_hover_skips_disabled_and_divider() {
        let mut m = three_items();
        m.set_hover(Some(2)); // divider
        assert_eq!(m.hovered, None);
        m.set_hover(Some(3)); // disabled
        assert_eq!(m.hovered, None);
        m.set_hover(Some(0));
        assert_eq!(m.hovered, Some(0));
    }
}
