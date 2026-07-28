//! Text widget — DirectWrite-rendered string with i18n support.
//!
//! Spec §10: `content: Cow<'static, str>` — both `&'static str` literals
//! (e.g. from i18n tables) and owned `String` (rare runtime-formatted text)
//! fit the same field with zero allocation for the literal path.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Color, Edges, Length, StringId};
use bentodesk_theme as theme;
use std::borrow::Cow;

/// Text payload. Default to `Cow::Borrowed("")` so struct-literal
/// construction stays ergonomic.
#[derive(Debug, Clone)]
pub struct TextNode {
    pub content: Cow<'static, str>,
    /// Optional i18n id. When set, the renderer reads the active locale via
    /// `bentodesk_style::t(id)` and ignores `content`. When `None`, the
    /// renderer uses `content` directly.
    pub id: Option<StringId>,
    /// 16pt is the default per spec §5.
    pub font_size_pt: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub color: Color,
    pub width: Length,
    pub height: Length,
}

impl TextNode {
    /// Construct from a literal or owned string. `&'static str` literals
    /// take the zero-alloc `Cow::Borrowed` path; `String` callers go
    /// through `Cow::Owned`.
    pub fn new(content: impl Into<Cow<'static, str>>) -> Self {
        let tokens = theme::current();
        Self {
            content: content.into(),
            id: None,
            font_size_pt: 16.0,
            font_weight: tokens.typo.weights.normal,
            line_height: tokens.typo.line_heights.normal,
            color: tokens.palette.text,
            width: Length::Auto,
            height: Length::Auto,
        }
    }

    /// Construct an i18n-driven text node. The renderer resolves `id`
    /// against the active locale at draw time, so the same node renders
    /// correctly across locale switches without rebuilding the tree.
    pub fn from_id(id: StringId) -> Self {
        let tokens = theme::current();
        Self {
            content: Cow::Borrowed(""),
            id: Some(id),
            font_size_pt: 16.0,
            font_weight: tokens.typo.weights.normal,
            line_height: tokens.typo.line_heights.normal,
            color: tokens.palette.text,
            width: Length::Auto,
            height: Length::Auto,
        }
    }

    /// Resolve the on-screen string. Reads the active locale when `id` is
    /// set; otherwise borrows `content`. The returned slice is valid for
    /// the lifetime of `self` (or `'static` for the i18n path).
    pub fn resolved(&self) -> &str {
        match self.id {
            Some(id) => bentodesk_style::t(id),
            None => &self.content,
        }
    }

    /// Encode the resolved text into a reusable UTF-16 buffer for
    /// DirectWrite. `out` is cleared and refilled — caller owns the
    /// allocation across frames (spec §10).
    pub fn encode_utf16_into(&self, out: &mut smallvec::SmallVec<[u16; 256]>) {
        out.clear();
        for u in self.resolved().encode_utf16() {
            out.push(u);
        }
    }
}

impl Default for TextNode {
    fn default() -> Self {
        let tokens = theme::current();
        Self {
            content: Cow::Borrowed(""),
            id: None,
            font_size_pt: 16.0,
            font_weight: tokens.typo.weights.normal,
            line_height: tokens.typo.line_heights.normal,
            color: tokens.palette.text,
            width: Length::Auto,
            height: Length::Auto,
        }
    }
}

impl LayoutSource for TextNode {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Column,
            width: self.width,
            height: self.height,
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentodesk_style::{ZH_CN, i18n_zh_cn::ids as zh_ids, init_locale};

    #[test]
    fn text_new_from_literal_is_borrowed() {
        let t = TextNode::new("Hello");
        assert!(matches!(t.content, Cow::Borrowed(_)));
        assert_eq!(t.resolved(), "Hello");
        assert!(t.id.is_none());
        assert_eq!(t.font_weight, theme::current().typo.weights.normal);
        assert_eq!(t.line_height, theme::current().typo.line_heights.normal);
    }

    #[test]
    fn text_from_id_resolves_via_active_locale() {
        init_locale(&ZH_CN);
        let t = TextNode::from_id(zh_ids::TOOLBAR_PIN);
        assert_eq!(t.resolved(), "钉住");
        assert_eq!(t.id, Some(zh_ids::TOOLBAR_PIN));
    }
}
