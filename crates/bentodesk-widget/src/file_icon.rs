//! `FileIcon` — file-system icon resolved via Win32 `IExtractIconW`. The
//! widget owns the file path + size hint + extracted hash; the renderer
//! looks the hash up in the platform's icon cache (`bentodesk-platform::
//! iextracticon` — populated by T-033 backend extraction).
//!
//! Spec §10: `path` is `Arc<Path>` — the widget shares the path handle with
//! the cache key without duplicating the OS string. `cache_hash` is `u64`,
//! cheap to copy.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{BorderRadius, Color, Edges, Length};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default 32×32 — matches Win32 `SHIL_LARGE` (32 px) for HiDPI 96-DPI
/// displays; the platform cache stores both 32 and 64 (`SHIL_EXTRALARGE`)
/// variants and the renderer picks based on DPI.
pub const DEFAULT_SIZE_PX: f32 = 32.0;

/// Sentinel used while the platform extraction is still in flight. Renderer
/// shows a placeholder square (or a generic file glyph from SvgIcon) when
/// the hash equals this value.
pub const PENDING_HASH: u64 = 0;

#[derive(Debug, Clone)]
pub struct FileIcon {
    pub path: Arc<PathBuf>,
    pub size: f32,
    /// Content hash of the extracted icon. Zero = pending; non-zero indexes
    /// the platform icon cache. Caller updates after the platform finishes
    /// `IExtractIconW::Extract`.
    pub cache_hash: u64,
    pub border_radius: BorderRadius,
    /// Background drawn behind the icon (for cards / placeholders).
    pub background: Color,
}

impl FileIcon {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            size: DEFAULT_SIZE_PX,
            cache_hash: PENDING_HASH,
            border_radius: BorderRadius::all(4.0),
            background: Color::TRANSPARENT,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Mark the icon as resolved with a non-zero `hash`. Renderer flips from
    /// placeholder to cached bitmap on next paint.
    pub fn set_hash(&mut self, hash: u64) {
        self.cache_hash = hash;
    }

    pub fn is_pending(&self) -> bool {
        self.cache_hash == PENDING_HASH
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl LayoutSource for FileIcon {
    fn layout(&self) -> LayoutDesc {
        LayoutDesc {
            direction: Direction::Row,
            width: Length::Px(self.size),
            height: Length::Px(self.size),
            padding: Edges::ZERO,
            ..LayoutDesc::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_icon_starts_pending() {
        let f = FileIcon::new("C:/Users/test/x.txt");
        assert!(f.is_pending());
        assert_eq!(f.cache_hash, PENDING_HASH);
    }

    #[test]
    fn file_icon_set_hash_flips_pending_off() {
        let mut f = FileIcon::new("C:/Users/test/x.txt");
        f.set_hash(0xdead_beef);
        assert!(!f.is_pending());
        assert_eq!(f.cache_hash, 0xdead_beef);
    }

    #[test]
    fn file_icon_path_returns_borrowed_path() {
        let f = FileIcon::new("C:/Users/test/x.txt");
        assert_eq!(f.path(), Path::new("C:/Users/test/x.txt"));
    }

    #[test]
    fn file_icon_with_size_propagates() {
        let f = FileIcon::new("x").with_size(64.0);
        assert!((f.size - 64.0).abs() < 1e-6);
    }
}
