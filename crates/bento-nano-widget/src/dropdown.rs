//! `Dropdown` — combobox that opens a popup HWND with a List of options. The
//! widget owns the option set + selected index; the runtime opens / closes
//! the popup via [`crate::popup::Popup`].

use bento_nano_layout::{Direction, LayoutDesc, LayoutSource};
use bento_nano_style::{BorderRadius, Color, Edges, Length, Size};
use bento_nano_theme as theme;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::popup::{Popup, PopupAnchor, PopupPlacement};

#[derive(Debug, Clone)]
pub struct DropdownOption {
    pub label: SmolStr,
    pub value_id: u32,
    pub disabled: bool,
}

impl DropdownOption {
    pub fn new(label: impl Into<SmolStr>, value_id: u32) -> Self {
        Self {
            label: label.into(),
            value_id,
            disabled: false,
        }
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

pub const ROW_HEIGHT_PX: f32 = 28.0;
pub const DEFAULT_HEIGHT_PX: f32 = 32.0;
pub const DEFAULT_WIDTH_PX: f32 = 200.0;

#[derive(Debug, Clone)]
pub struct Dropdown {
    pub options: SmallVec<[DropdownOption; 8]>,
    /// Selected option's value_id, or 0 = no selection.
    pub selected_value: u32,
    pub width: f32,
    pub height: f32,
    pub padding: Edges,
    pub disabled: bool,
    pub popup: Popup,
    pub on_change_event: u32,
    pub background: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text: Color,
    pub radius: BorderRadius,
}

impl Dropdown {
    pub fn new(options: impl IntoIterator<Item = DropdownOption>, on_change_event: u32) -> Self {
        let p = theme::current().palette;
        let options: SmallVec<[DropdownOption; 8]> = options.into_iter().collect();
        let popup_height = options.len() as f32 * ROW_HEIGHT_PX + 8.0;
        let popup = Popup::new(Size {
            width: DEFAULT_WIDTH_PX,
            height: popup_height,
        });
        Self {
            options,
            selected_value: 0,
            width: DEFAULT_WIDTH_PX,
            height: DEFAULT_HEIGHT_PX,
            padding: Edges::xy(8.0, 6.0),
            disabled: false,
            popup,
            on_change_event,
            background: p.surface_alt,
            border: p.border,
            border_focus: p.accent,
            text: p.text,
            radius: BorderRadius::all(4.0),
        }
    }

    pub fn open(&mut self, anchor: PopupAnchor) {
        if self.disabled {
            return;
        }
        self.popup.anchor = anchor;
        self.popup.placement = PopupPlacement::Bottom;
        self.popup.show();
    }

    pub fn close(&mut self) {
        self.popup.hide();
    }

    pub fn is_open(&self) -> bool {
        self.popup.visible
    }

    /// Pick `index`-th option. No-op when out-of-range or disabled.
    pub fn select_index(&mut self, index: u32) -> bool {
        let usz = index as usize;
        let opt = match self.options.get(usz) {
            Some(o) => o,
            None => return false,
        };
        if opt.disabled || opt.value_id == self.selected_value {
            return false;
        }
        self.selected_value = opt.value_id;
        self.close();
        true
    }

    pub fn selected_option(&self) -> Option<&DropdownOption> {
        self.options
            .iter()
            .find(|o| o.value_id == self.selected_value)
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.selected_option().map(|o| o.label.as_str())
    }

    pub fn emit<F: FnMut(u32, u32)>(&self, mut sink: F) -> bool {
        if self.on_change_event == 0 || self.selected_value == 0 {
            return false;
        }
        sink(self.on_change_event, self.selected_value);
        true
    }
}

impl LayoutSource for Dropdown {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.width),
            height: Length::Px(self.height),
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_style::Rect;

    fn three() -> Dropdown {
        Dropdown::new(
            [
                DropdownOption::new("Dark", 1),
                DropdownOption::new("Light", 2),
                DropdownOption::new("System", 3).disabled(true),
            ],
            10,
        )
    }

    #[test]
    fn dropdown_open_shows_popup_with_anchor() {
        let mut d = three();
        d.open(Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 32.0,
        });
        assert!(d.is_open());
        assert_eq!(d.popup.anchor.x, 100.0);
    }

    #[test]
    fn dropdown_select_index_updates_value_and_closes() {
        let mut d = three();
        d.open(Rect::ZERO);
        let ok = d.select_index(1);
        assert!(ok);
        assert_eq!(d.selected_value, 2);
        assert!(!d.is_open());
        assert_eq!(d.selected_label(), Some("Light"));
    }

    #[test]
    fn dropdown_select_disabled_index_is_noop() {
        let mut d = three();
        let ok = d.select_index(2);
        assert!(!ok);
        assert_eq!(d.selected_value, 0);
    }

    #[test]
    fn dropdown_select_oob_index_is_noop() {
        let mut d = three();
        let ok = d.select_index(99);
        assert!(!ok);
    }

    #[test]
    fn dropdown_disabled_open_is_noop() {
        let mut d = three();
        d.disabled = true;
        d.open(Rect::ZERO);
        assert!(!d.is_open());
    }
}
