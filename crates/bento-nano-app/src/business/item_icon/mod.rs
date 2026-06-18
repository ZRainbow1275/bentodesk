//! Business surface — `ItemIcon`, the icon slot inside an `ItemCard`.
//!
//! Visual spec: see `item_icon.snap.md`. Geometry is locked; the
//! selected-stack runtime fallback maps extensions to the same line-art
//! `IconKind` family used by real zone icons. The legacy `fallback_emoji_for`
//! table is retained only for compatibility tests around the old 1.x map.
//!
//! Status: scaffolding per Wave E Option-A. The `build()` returns the
//! outer Container; the inner image / glyph composition lands when
//! widget-library ships `FileIcon`. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::Length;
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use crate::business::icons::IconKind;

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
    /// Render the extension-keyed selected-stack line-art fallback.
    Error,
}

/// Margin (logical px) outside the viewport at which the lazy IO observer
/// starts warming up an icon. Mirrors 1.x `PRELOAD_ROOT_MARGIN = "200px"`.
pub const ICON_PRELOAD_MARGIN_PX: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FallbackIconFamily {
    Folder,
    Document,
    TextDocument,
    Spreadsheet,
    Image,
    Video,
    Audio,
    Code,
    Archive,
    Executable,
    Shortcut,
}

impl FallbackIconFamily {
    const fn icon_kind(self) -> IconKind {
        match self {
            Self::Folder => IconKind::Folder,
            Self::Document | Self::TextDocument => IconKind::Document,
            Self::Spreadsheet => IconKind::Grid,
            Self::Image => IconKind::Image,
            Self::Video => IconKind::Video,
            Self::Audio => IconKind::Music,
            Self::Code => IconKind::Code,
            Self::Archive => IconKind::Archive,
            Self::Executable => IconKind::Settings,
            Self::Shortcut => IconKind::ExternalLink,
        }
    }

    const fn legacy_emoji(self) -> &'static str {
        match self {
            Self::Folder => "\u{1F4C1}",
            Self::Document => "\u{1F4C4}",
            Self::TextDocument => "\u{1F4C3}",
            Self::Spreadsheet => "\u{1F4CA}",
            Self::Image => "\u{1F5BC}",
            Self::Video => "\u{1F3AC}",
            Self::Audio => "\u{1F3B5}",
            Self::Code => "\u{1F4BB}",
            Self::Archive => "\u{1F4E6}",
            Self::Executable => "\u{2699}",
            Self::Shortcut => "\u{1F517}",
        }
    }
}

fn fallback_icon_family_for(path: &str) -> FallbackIconFamily {
    let ext = path
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && *e != path);
    let Some(ext) = ext else {
        return FallbackIconFamily::Folder;
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

    match lower {
        // documents
        "doc" | "docx" | "pdf" => FallbackIconFamily::Document,
        "txt" | "md" | "rtf" => FallbackIconFamily::TextDocument,
        // spreadsheets / presentations
        "xlsx" | "xls" | "csv" | "pptx" | "ppt" => FallbackIconFamily::Spreadsheet,
        // images
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" => {
            FallbackIconFamily::Image
        }
        // video
        "mp4" | "avi" | "mkv" | "mov" | "wmv" | "webm" => FallbackIconFamily::Video,
        // audio
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => FallbackIconFamily::Audio,
        // code
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "java" | "cpp" | "c" | "h" | "cs"
        | "html" | "css" => FallbackIconFamily::Code,
        // archives
        "zip" | "rar" | "7z" | "tar" | "gz" => FallbackIconFamily::Archive,
        // executables / scripts
        "exe" | "msi" | "bat" | "cmd" | "ps1" => FallbackIconFamily::Executable,
        // shortcuts
        "lnk" | "url" => FallbackIconFamily::Shortcut,
        _ => FallbackIconFamily::Folder,
    }
}

/// Map a file path's extension to the selected-stack line-art fallback icon.
///
/// The lookup is ASCII-case-insensitive. Missing or unknown extensions return
/// [`IconKind::Folder`], matching the legacy 1.x default category without
/// painting an emoji glyph in the native renderer.
pub fn fallback_icon_kind_for(path: &str) -> IconKind {
    fallback_icon_family_for(path).icon_kind()
}

/// Map a file path's extension to the legacy 1.x fallback emoji.
///
/// The selected-stack runtime should use [`fallback_icon_kind_for`] instead.
pub fn fallback_emoji_for(path: &str) -> SmolStr {
    SmolStr::new_inline(fallback_icon_family_for(path).legacy_emoji())
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
    fn fallback_icon_kind_uses_line_art_categories() {
        assert_eq!(fallback_icon_kind_for(""), IconKind::Folder);
        assert_eq!(fallback_icon_kind_for("foo.unknownext"), IconKind::Folder);
        assert_eq!(fallback_icon_kind_for("a.pdf"), IconKind::Document);
        assert_eq!(fallback_icon_kind_for("notes.MD"), IconKind::Document);
        assert_eq!(fallback_icon_kind_for("sheet.csv"), IconKind::Grid);
        assert_eq!(fallback_icon_kind_for("p.png"), IconKind::Image);
        assert_eq!(fallback_icon_kind_for("v.MP4"), IconKind::Video);
        assert_eq!(fallback_icon_kind_for("a.mp3"), IconKind::Music);
        assert_eq!(fallback_icon_kind_for("src.rs"), IconKind::Code);
        assert_eq!(fallback_icon_kind_for("pack.zip"), IconKind::Archive);
        assert_eq!(fallback_icon_kind_for("setup.exe"), IconKind::Settings);
        assert_eq!(
            fallback_icon_kind_for("shortcut.lnk"),
            IconKind::ExternalLink
        );
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
