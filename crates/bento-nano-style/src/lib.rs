#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bento-nano-style` — style primitives.
//!
//! Spec §3.2: 100% self-rolled; no third-party UI crates, no CSS parser.
//! Spec §10: hot-path types are `Copy` + small; no `String` fields.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod i18n;
pub mod i18n_en_us;
pub mod i18n_zh_cn;
pub mod lerp;
pub mod tokens;

pub use i18n::{LookupTable, StringId, current_locale_is, init_locale, set_locale, t};
pub use i18n_en_us::EN_US;
pub use i18n_zh_cn::ZH_CN;
pub use lerp::Lerp;

/// 32-bit linear-RGBA colour, 0..=1 floats. Premultiply at the brush boundary
/// (see `bento-nano-platform::d2d::solid_brush`), not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Construct from 8-bit sRGB tuple. Accepts integer literals like
    /// `Color::from_u8(0x18, 0x18, 0x1C, 0xCC)`. No gamma decode — we treat
    /// inputs as already-linear which matches the React design tokens.
    pub const fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }
}

/// Length unit. `Px` is device-independent pixels at 96 DPI; the renderer
/// applies the per-monitor scale factor at paint time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    /// Fraction of parent main-axis length, 0..=1.
    Fraction(f32),
    /// Auto = let layout decide (intrinsic / flex grow).
    Auto,
}

impl Length {
    pub const ZERO: Length = Length::Px(0.0);

    /// Resolve `self` against a parent main-axis size in DIPs. `Auto` resolves
    /// to 0.0 — callers that need intrinsic sizing must check the variant.
    pub fn resolve(self, parent: f32) -> f32 {
        match self {
            Length::Px(v) => v,
            Length::Fraction(f) => parent * f,
            Length::Auto => 0.0,
        }
    }
}

/// Per-side edge measurements (padding, margin, border).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Edges = Edges {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub const fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            top: y,
            right: x,
            bottom: y,
            left: x,
        }
    }

    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// Border-radius (per-corner). Equal radii share one value via [`BorderRadius::all`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    pub const ZERO: BorderRadius = BorderRadius {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    pub const fn all(v: f32) -> Self {
        Self {
            top_left: v,
            top_right: v,
            bottom_right: v,
            bottom_left: v,
        }
    }
}

/// Drop shadow descriptor. Renderer wires this to D2D `CLSID_D2D1Shadow` when
/// the `shadow` feature is enabled in `bento-nano-platform`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub color: Color,
}

impl Shadow {
    pub const NONE: Shadow = Shadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        color: Color::TRANSPARENT,
    };
}

/// Logical 2D rectangle in DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn right(&self) -> f32 {
        self.x + self.width
    }

    pub const fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// 2D size in DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };
}

// -----------------------------------------------------------------------------
// Phase 2.3.1b — DPI scaling math (single conversion site).
//
// `bento-nano-style` is the canonical home for these helpers because:
//   * Layout (`bento-nano-layout`) and the renderer (`bento-nano-app::render`)
//     both depend on `style`, so neither needs a new edge.
//   * The functions are pure float / integer arithmetic — no Win32, no COM —
//     so the leaf `style` crate stays platform-agnostic (§13' R1 layer rule).
//   * Co-locating `Length` / `Rect` / `Size` (DIP-typed) with the conversion
//     functions makes "logical vs device" an in-file concern, not a doc-only
//     contract scattered across crates.
//
// Direction policy (single source of truth):
//   * Layout, hit-testing, and zone geometry live in **logical** units (DIPs).
//   * D2D draws via a single `SetTransform(Scale)` so logical coords land at
//     the right device pixel without per-call multiplication.
//   * Mouse / WM_NCHITTEST input arrives in **device** pixels and crosses the
//     boundary via `device_to_logical_f32` exactly once per event.
// -----------------------------------------------------------------------------

pub mod dpi {
    //! DPI scaling math — Phase 2.3.1b.
    //!
    //! `BASE_DPI` is the Win32 `USER_DEFAULT_SCREEN_DPI` (96) — every logical
    //! coordinate in BentoDesk is anchored to this baseline. The functions
    //! below convert between logical (DIP) and device (physical pixel) space
    //! using `dpi / 96` as the scale factor.

    use super::Size;

    /// Win32 `USER_DEFAULT_SCREEN_DPI` (100% scale).
    pub const BASE_DPI: u32 = 96;

    /// Scale factor applied to logical coordinates to obtain device pixels.
    ///
    /// `dpi == 96` returns `1.0` (identity); `dpi == 192` returns `2.0`.
    /// Defends against `dpi == 0` by treating it as 96 — matches the
    /// `GetDpiForWindow` fallback in `paint()`.
    #[inline]
    pub fn scale_factor(dpi: u32) -> f32 {
        let safe = if dpi == 0 { BASE_DPI } else { dpi };
        (safe as f32) / (BASE_DPI as f32)
    }

    /// Convert one device-pixel scalar to its logical-space equivalent.
    /// Used by the wndproc to translate WM_MOUSEMOVE / WM_LBUTTONDOWN
    /// / WM_NCHITTEST coordinates into the logical space the layout +
    /// zone collections live in.
    #[inline]
    pub fn device_to_logical_f32(device: f32, dpi: u32) -> f32 {
        device / scale_factor(dpi)
    }

    /// Convert a device-pixel `Size` (typically the swap chain backbuffer
    /// dimensions reported by `WM_SIZE` / `GetClientRect`) to the logical
    /// `Size` the layout engine consumes as `viewport`.
    #[inline]
    pub fn device_size_to_logical(device: Size, dpi: u32) -> Size {
        let s = scale_factor(dpi);
        Size {
            width: device.width / s,
            height: device.height / s,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn scale_factor_at_96_dpi_is_identity() {
            assert!((scale_factor(96) - 1.0).abs() < 1e-6);
        }

        #[test]
        fn scale_factor_at_192_dpi_is_two() {
            assert!((scale_factor(192) - 2.0).abs() < 1e-6);
        }

        #[test]
        fn scale_factor_treats_zero_as_baseline() {
            // Defends against a `GetDpiForWindow` zero return that slipped
            // past the shell's fallback. `scale_factor(0)` must NOT divide
            // through zero anywhere downstream.
            assert!((scale_factor(0) - 1.0).abs() < 1e-6);
        }

        #[test]
        fn device_to_logical_at_96_is_identity() {
            // 96 DPI must be a no-op so existing 96-DPI deployments stay
            // bit-for-bit identical pre / post Phase 2.3.1b.
            assert!((device_to_logical_f32(123.0, 96) - 123.0).abs() < 1e-6);
        }

        #[test]
        fn device_size_to_logical_at_192_halves() {
            let logical = device_size_to_logical(
                Size {
                    width: 960.0,
                    height: 640.0,
                },
                192,
            );
            assert!((logical.width - 480.0).abs() < 1e-6);
            assert!((logical.height - 320.0).abs() < 1e-6);
        }
    }
}
