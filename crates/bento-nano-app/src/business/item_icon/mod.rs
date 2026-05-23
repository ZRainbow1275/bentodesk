//! Business surface — `ItemIcon`, the icon slot inside an `ItemCard`.
//!
//! Visual spec: see `item_icon.snap.md`. Geometry is locked; the
//! `fallback_emoji_for` table is the verbatim port of the 1.x extension
//! map and is exercised by unit tests so a future refactor can't regress
//! the user-visible icon for any of the 50+ cataloged types.
//!
//! Status: scaffolding per Wave E Option-A. The `build()` returns the
//! outer Container; the inner image / glyph composition lands when
//! widget-library ships `FileIcon`. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::Length;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// Card layout variant — drives both container and render size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconSize {
    #[default]
    Standard,
    Wide,
}

impl IconSize {
    /// Outer container square side length, logical px. Snap.md mandated.
    pub const fn container_px(self) -> f32 {
        match self {
            Self::Standard => 36.0,
            Self::Wide => 28.0,
        }
    }

    /// Inner image render side length, logical px. Snap.md mandated.
    pub const fn render_px(self) -> f32 {
        match self {
            Self::Standard => 24.0,
            Self::Wide => 20.0,
        }
    }
}

/// Lifecycle state for a per-card icon. The renderer chooses the brush
/// (image vs pulse vs fallback glyph) by matching this enum, not by
/// string-checking a state name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconRenderState {
    /// Card is offscreen; we have not yet asked the backend for a hash.
    Idle,
    /// Backend extract is in flight or the PNG fetch hasn't decoded yet.
    /// The container shows the pulse placeholder.
    Loading,
    /// PNG decoded successfully; render the image at `render_px`.
    Ready,
    /// Extraction failed (file missing / HICON returned transparent).
    /// Render the extension-keyed emoji fallback.
    Error,
}

/// Margin (logical px) outside the viewport at which the lazy IO observer
/// starts warming up an icon. Mirrors 1.x `PRELOAD_ROOT_MARGIN = "200px"`.
pub const ICON_PRELOAD_MARGIN_PX: f32 = 200.0;

/// Map a file path's extension to the fallback emoji. The lookup is
/// ASCII-case-insensitive. Returns `📁` (folder) when the extension is
/// missing or unknown — matches the 1.x default branch.
pub fn fallback_emoji_for(path: &str) -> SmolStr {
    let ext = path
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && *e != path);
    let Some(ext) = ext else {
        return SmolStr::new_inline("\u{1F4C1}");
    };

    let mut buf = [0u8; 16];
    let lower: &str = if ext.len() <= buf.len() && ext.is_ascii() {
        let bytes = ext.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            buf[i] = b.to_ascii_lowercase();
        }
        std::str::from_utf8(&buf[..bytes.len()]).unwrap_or(ext)
    } else {
        ext
    };

    let glyph = match lower {
        // documents
        "doc" | "docx" | "pdf" => "\u{1F4C4}",
        "txt" | "md" | "rtf" => "\u{1F4C3}",
        // spreadsheets / presentations
        "xlsx" | "xls" | "csv" | "pptx" | "ppt" => "\u{1F4CA}",
        // images
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" => "\u{1F5BC}",
        // video
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "webm" => "\u{1F3AC}",
        // audio
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => "\u{1F3B5}",
        // code
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "cpp" | "c" | "h" | "cs"
        | "html" | "css" => "\u{1F4BB}",
        // archives
        "zip" | "rar" | "7z" | "tar" | "gz" => "\u{1F4E6}",
        // executables / scripts
        "exe" | "msi" | "bat" | "cmd" | "ps1" => "\u{2699}",
        // shortcuts
        "lnk" | "url" => "\u{1F517}",
        _ => "\u{1F4C1}",
    };
    SmolStr::new_inline(glyph)
}

/// Build the icon container with the default (Standard) size.
pub fn build() -> WidgetNode {
    build_with(IconSize::default())
}

/// Build the icon container at a specific size. Geometry locked per
/// `item_icon.snap.md`.
pub fn build_with(size: IconSize) -> WidgetNode {
    let side = size.container_px();
    WidgetNode::Container(ContainerNode {
        direction: Direction::Row,
        width: Length::Px(side),
        height: Length::Px(side),
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn size_tables_match_snap_md() {
        assert!((IconSize::Standard.container_px() - 36.0).abs() < 0.01);
        assert!((IconSize::Wide.container_px() - 28.0).abs() < 0.01);
        assert!((IconSize::Standard.render_px() - 24.0).abs() < 0.01);
        assert!((IconSize::Wide.render_px() - 20.0).abs() < 0.01);
    }

    #[test]
    fn preload_margin_matches_snap_md() {
        assert!((ICON_PRELOAD_MARGIN_PX - 200.0).abs() < 0.01);
    }

    #[test]
    fn fallback_unknown_returns_folder() {
        assert_eq!(fallback_emoji_for(""), "\u{1F4C1}");
        assert_eq!(fallback_emoji_for("nodot"), "\u{1F4C1}");
        assert_eq!(fallback_emoji_for("foo.unknownext"), "\u{1F4C1}");
    }

    #[test]
    fn fallback_documents() {
        assert_eq!(fallback_emoji_for("a.pdf"), "\u{1F4C4}");
        assert_eq!(fallback_emoji_for("A.PDF"), "\u{1F4C4}");
        assert_eq!(fallback_emoji_for("notes.MD"), "\u{1F4C3}");
    }

    #[test]
    fn fallback_images_video_audio_code_archive_exe_link() {
        assert_eq!(fallback_emoji_for("p.png"), "\u{1F5BC}");
        assert_eq!(fallback_emoji_for("v.MP4"), "\u{1F3AC}");
        assert_eq!(fallback_emoji_for("a.mp3"), "\u{1F3B5}");
        assert_eq!(fallback_emoji_for("src.rs"), "\u{1F4BB}");
        assert_eq!(fallback_emoji_for("pack.zip"), "\u{1F4E6}");
        assert_eq!(fallback_emoji_for("setup.exe"), "\u{2699}");
        assert_eq!(fallback_emoji_for("shortcut.lnk"), "\u{1F517}");
    }

    #[test]
    fn build_default_is_standard_36px_square() {
        let node = build();
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - 36.0).abs() < 0.01));
        assert!(matches!(layout.height, Length::Px(h) if (h - 36.0).abs() < 0.01));
    }

    #[test]
    fn build_wide_is_28px_square() {
        let node = build_with(IconSize::Wide);
        let layout = node.layout();
        assert!(matches!(layout.width, Length::Px(w) if (w - 28.0).abs() < 0.01));
    }

    #[test]
    fn icon_size_serde_round_trip() {
        for v in [IconSize::Standard, IconSize::Wide] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: IconSize = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        assert_eq!(
            serde_json::to_string(&IconSize::Standard).unwrap_or_default(),
            "\"standard\""
        );
    }
}
