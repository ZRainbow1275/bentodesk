//! Image widget — SVG path or WIC-backed file bitmap.
//!
//! File-backed images are decoded by the renderer through the platform WIC
//! bridge. No `image` crate, no bundled codec, no mock pixel payload.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Color, Edges, Length};
use smol_str::SmolStr;

/// Source descriptor for either inline vector paths or real files on disk.
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Embedded SVG `<path d="...">` data; rendered via `bentodesk-platform::svg`.
    SvgPath(SmolStr),
    /// File path on disk; renderer decodes through WIC and caches the bitmap.
    File(SmolStr),
}

#[derive(Debug, Clone)]
pub struct ImageNode {
    pub source: ImageSource,
    pub width: Length,
    pub height: Length,
    /// Optional tint (premultiplied at the brush boundary, not here).
    pub tint: Color,
}

impl ImageNode {
    pub fn from_svg_path(path: impl Into<SmolStr>) -> Self {
        Self {
            source: ImageSource::SvgPath(path.into()),
            width: Length::Px(24.0),
            height: Length::Px(24.0),
            tint: Color::BLACK,
        }
    }

    pub fn from_file(path: impl Into<SmolStr>, width: f32, height: f32) -> Self {
        Self {
            source: ImageSource::File(path.into()),
            width: Length::Px(width.max(1.0)),
            height: Length::Px(height.max(1.0)),
            tint: Color::BLACK,
        }
    }
}

impl Default for ImageNode {
    fn default() -> Self {
        Self {
            source: ImageSource::SvgPath(SmolStr::new_static("")),
            width: Length::Px(24.0),
            height: Length::Px(24.0),
            tint: Color::BLACK,
        }
    }
}

impl LayoutSource for ImageNode {
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

    #[test]
    fn file_image_source_preserves_path_and_clamps_size() {
        let image = ImageNode::from_file("C:/tmp/example.png", 0.0, -4.0);
        match image.source {
            ImageSource::File(path) => assert_eq!(path.as_str(), "C:/tmp/example.png"),
            ImageSource::SvgPath(_) => panic!("expected file-backed image source"),
        }
        assert_eq!(image.width, Length::Px(1.0));
        assert_eq!(image.height, Length::Px(1.0));
    }
}
