//! `SvgIcon` — SVG icon rendered through `bentodesk-platform::svg`. Holds
//! the path data + a content-addressed cache key so the platform's SVG cache
//! (T-047 LRU) can reuse parsed geometries across frames and across widgets
//! that reference the same icon.
//!
//! Spec §10: path data is `&'static str` so Lucide icons stay in the binary
//! as compile-time literals. Custom user icons go through `SmolStr` for the
//! short-path case + spill to heap for long ones.

use bentodesk_layout::{Direction, LayoutDesc, LayoutSource};
use bentodesk_style::{Color, Edges, Length};
use bentodesk_theme as theme;
use smol_str::SmolStr;

#[derive(Debug, Clone)]
pub enum SvgSource {
    /// Compile-time icon (Lucide). Cache key is the pointer identity.
    Static(&'static str),
    /// Runtime icon (custom upload). Cache key is the SmolStr's hash.
    Dynamic(SmolStr),
}

impl SvgSource {
    /// Stable identity used by the platform SVG cache. For static literals
    /// the pointer address is used; for dynamic strings the hash of the
    /// content is used.
    pub fn cache_key(&self) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = SimpleHasher::default();
        match self {
            SvgSource::Static(s) => {
                // Pointer-as-int — stable within a single process run.
                let ptr = s.as_ptr() as usize as u64;
                ptr.hash(&mut h);
            }
            SvgSource::Dynamic(s) => {
                s.as_str().hash(&mut h);
            }
        }
        h.finish()
    }

    pub fn as_str(&self) -> &str {
        match self {
            SvgSource::Static(s) => s,
            SvgSource::Dynamic(s) => s.as_str(),
        }
    }
}

/// Tiny FNV-1a — avoids pulling `std::collections::hash_map::DefaultHasher`
/// (which depends on `RandomState`'s ad-hoc PRNG init at construction).
#[derive(Default)]
struct SimpleHasher(u64);

impl core::hash::Hasher for SimpleHasher {
    fn write(&mut self, bytes: &[u8]) {
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut h = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        self.0 = h;
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct SvgIcon {
    pub source: SvgSource,
    pub size: f32,
    pub tint: Color,
}

impl SvgIcon {
    pub fn from_static(path_d: &'static str) -> Self {
        let p = theme::current().palette;
        Self {
            source: SvgSource::Static(path_d),
            size: 24.0,
            tint: p.text,
        }
    }

    pub fn from_smol(path_d: impl Into<SmolStr>) -> Self {
        let p = theme::current().palette;
        Self {
            source: SvgSource::Dynamic(path_d.into()),
            size: 24.0,
            tint: p.text,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
}

impl LayoutSource for SvgIcon {
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

    const HOME_PATH: &str = "M12 3L4 9v12h16V9z";
    const SETTINGS_PATH: &str = "M12 8a4 4 0 100 8 4 4 0 000-8z";

    #[test]
    fn svg_icon_static_default_size_is_24() {
        let i = SvgIcon::from_static(HOME_PATH);
        assert!((i.size - 24.0).abs() < 1e-6);
    }

    #[test]
    fn svg_icon_cache_key_differs_between_distinct_static_paths() {
        let a = SvgIcon::from_static(HOME_PATH);
        let b = SvgIcon::from_static(SETTINGS_PATH);
        assert_ne!(a.source.cache_key(), b.source.cache_key());
    }

    #[test]
    fn svg_icon_cache_key_stable_for_same_dynamic_content() {
        let a = SvgIcon::from_smol("M0 0L1 1");
        let b = SvgIcon::from_smol("M0 0L1 1");
        assert_eq!(a.source.cache_key(), b.source.cache_key());
    }

    #[test]
    fn svg_icon_with_size_and_tint_propagate() {
        let i = SvgIcon::from_static(HOME_PATH)
            .with_size(40.0)
            .with_tint(Color::WHITE);
        assert!((i.size - 40.0).abs() < 1e-6);
        assert_eq!(i.tint, Color::WHITE);
    }
}
