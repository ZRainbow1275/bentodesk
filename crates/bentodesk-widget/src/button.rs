//! Button widget — interactive container that emits a `command_id` on click.
//!
//! Colours are pulled from [`bentodesk_theme::current`] at construction time
//! so swapping `DARK_DEFAULT` ↔ `LIGHT_DEFAULT` via [`bentodesk_theme::set_current`]
//! is honoured by every freshly-built button. Re-paint of already-built
//! buttons goes through the T-003 subscriber/effect mechanism.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use bentodesk_theme as theme;
use smol_str::SmolStr;

/// Button payload. The label is owned `SmolStr`; the `command_id` is a stable
/// app-defined token the dispatcher routes to a handler.
#[derive(Debug, Clone)]
pub struct ButtonNode {
    pub label: SmolStr,
    pub command_id: SmolStr,
    pub width: Length,
    pub height: Length,
    pub padding: Edges,
    pub background: Color,
    pub label_color: Color,
    pub radius: BorderRadius,
    pub disabled: bool,
}

impl ButtonNode {
    pub fn new(label: impl Into<SmolStr>, command_id: impl Into<SmolStr>) -> Self {
        let palette = theme::current().palette;
        Self {
            label: label.into(),
            command_id: command_id.into(),
            width: Length::Auto,
            height: Length::Px(32.0),
            padding: Edges::xy(12.0, 6.0),
            background: palette.accent,
            label_color: Color::WHITE,
            radius: BorderRadius::all(6.0),
            disabled: false,
        }
    }
}

impl Default for ButtonNode {
    fn default() -> Self {
        Self::new("", "")
    }
}

impl LayoutSource for ButtonNode {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: self.width,
            height: self.height,
            padding: self.padding,
            ..LayoutDesc::default()
        }
    }
}
