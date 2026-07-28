#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
//! `bentodesk-style` — style primitives.
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
/// (see `bentodesk-platform::d2d::solid_brush`), not here.
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
/// the `shadow` feature is enabled in `bentodesk-platform`.
///
/// M6b — gained a `spread` field (the CSS `box-shadow` 4th length). Most tokens
/// use `spread == 0.0`; the `terminal` theme's `0 0 0 1px` ring is `spread == 1`,
/// `blur == 0` — a hairline outline with no soft falloff. The simulated soft-fill
/// render path grows the rect by `blur + spread` (see `render.rs::draw_shadow_stack`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    /// CSS `box-shadow` 4th length. 0 for every drop/glow token; `terminal`'s
    /// ring uses `spread == 1.0` with `blur == 0.0`.
    pub spread: f32,
    pub color: Color,
}

impl Shadow {
    pub const NONE: Shadow = Shadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: Color::TRANSPARENT,
    };

    /// Back-compat const ctor for the common `spread == 0.0` drop shadow, so a
    /// call-site that wants the pre-M6b 4-field shape stays one line. Equivalent
    /// to `Shadow { offset_x, offset_y, blur, spread: 0.0, color }`.
    pub const fn drop(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread: 0.0,
            color,
        }
    }
}

/// Multi-layer shadow stack. A fixed `[Shadow; 2]` inline array + a `len`
/// counter — covers every Tauri `presets.ts` shadow token (each has ≤2
/// comma-separated CSS layers: the Rounded group's 2-layer drop, `neo`'s
/// dual opposite-offset extrude, `cyberpunk`'s 2-colour glow, `order`'s single
/// flat layer, and the Angular `none` themes' empty stack).
///
/// `Copy`, stack-only, zero-alloc (§10) — deliberately NOT a `SmallVec` so the
/// leaf `style` crate stays free of the `smallvec` dep (§8). The 2-layer cap is
/// sufficient for all 51 builtin tokens; a hypothetical 3-layer custom theme
/// would have its 3rd layer dropped by the const builders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStack {
    layers: [Shadow; 2],
    /// 0 = none (flat/brutalism/editorial), 1 = single layer (order), 2 = dual.
    len: u8,
}

impl ShadowStack {
    /// Empty stack — paints no shadow (the Angular `none` themes).
    pub const NONE: ShadowStack = ShadowStack {
        layers: [Shadow::NONE, Shadow::NONE],
        len: 0,
    };

    /// Single-layer stack (e.g. `order`'s `0 1px 3px`).
    pub const fn one(a: Shadow) -> Self {
        Self {
            layers: [a, Shadow::NONE],
            len: 1,
        }
    }

    /// Two-layer stack. Drawn back-to-front: `a` (the inner / surface lift) is
    /// painted first, `b` (the outer / dominant drop) on top.
    pub const fn two(a: Shadow, b: Shadow) -> Self {
        Self {
            layers: [a, b],
            len: 2,
        }
    }

    /// The active layers in back-to-front draw order. Length is `0..=2`.
    pub fn layers(&self) -> &[Shadow] {
        let n = (self.len as usize).min(2);
        &self.layers[..n]
    }

    /// `true` when the stack paints nothing (the Angular `none` themes).
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of active layers (`0..=2`).
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// The dominant (outermost) layer — `layers()[len-1]`. Empty stacks return
    /// [`Shadow::NONE`]. Used by single-`Shadow` chrome consumers that paint one
    /// soft drop; for `dark` this equals the pre-M6b `SHADOW.expanded` / `.zen`
    /// outer layer byte-for-byte (the §5.3 byte-parity contract).
    pub const fn outer(&self) -> Shadow {
        match self.len {
            0 => Shadow::NONE,
            1 => self.layers[0],
            _ => self.layers[1],
        }
    }

    /// The innermost (first-drawn) layer — `layers()[0]`. Empty stacks return
    /// [`Shadow::NONE`]. For `dark` this equals the pre-M6b `*_inner` layer.
    pub const fn inner(&self) -> Shadow {
        match self.len {
            0 => Shadow::NONE,
            _ => self.layers[0],
        }
    }
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
// `bentodesk-style` is the canonical home for these helpers because:
//   * Layout (`bentodesk-layout`) and the renderer (`bentodesk-app::render`)
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

// -----------------------------------------------------------------------------
// M6b — Shadow `spread` + `ShadowStack` pure tests (no GPU/HWND — run under
// `cargo test -p bentodesk-style --lib`).
// -----------------------------------------------------------------------------

#[cfg(test)]
mod shadow_stack_tests {
    use super::{Color, Shadow, ShadowStack};

    #[test]
    fn drop_ctor_sets_spread_zero() {
        let s = Shadow::drop(0.0, 8.0, 32.0, Color::BLACK);
        assert_eq!(s.spread, 0.0);
        assert_eq!(s.offset_y, 8.0);
        assert_eq!(s.blur, 32.0);
        assert_eq!(s.color, Color::BLACK);
    }

    #[test]
    fn none_shadow_has_spread_zero_and_transparent() {
        assert_eq!(Shadow::NONE.spread, 0.0);
        assert_eq!(Shadow::NONE.color, Color::TRANSPARENT);
    }

    #[test]
    fn stack_none_is_empty() {
        assert!(ShadowStack::NONE.is_empty());
        assert_eq!(ShadowStack::NONE.len(), 0);
        assert_eq!(ShadowStack::NONE.layers().len(), 0);
        // Empty stack convenience accessors fall back to NONE.
        assert_eq!(ShadowStack::NONE.outer(), Shadow::NONE);
        assert_eq!(ShadowStack::NONE.inner(), Shadow::NONE);
    }

    #[test]
    fn stack_one_has_single_layer() {
        let a = Shadow::drop(0.0, 1.0, 3.0, Color::from_u8(0, 0, 0, 0x14));
        let s = ShadowStack::one(a);
        assert!(!s.is_empty());
        assert_eq!(s.len(), 1);
        assert_eq!(s.layers().len(), 1);
        // outer == inner == the only layer.
        assert_eq!(s.outer(), a);
        assert_eq!(s.inner(), a);
    }

    #[test]
    fn stack_two_orders_back_to_front() {
        // `a` is the inner (drawn first), `b` the outer (dominant drop).
        let a = Shadow::drop(0.0, 2.0, 8.0, Color::from_u8(0, 0, 0, 0x26));
        let b = Shadow::drop(0.0, 8.0, 32.0, Color::from_u8(0, 0, 0, 0x40));
        let s = ShadowStack::two(a, b);
        assert_eq!(s.len(), 2);
        assert_eq!(s.layers(), &[a, b]); // back-to-front
        assert_eq!(s.inner(), a);
        assert_eq!(s.outer(), b);
    }

    #[test]
    fn ring_layer_uses_spread_not_blur() {
        // terminal's `0 0 0 1px` ring: spread=1, blur=0 — a hairline outline.
        let ring = Shadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 1.0,
            color: Color::from_u8(0x00, 0xFF, 0x9C, 0x40),
        };
        assert_eq!(ring.blur, 0.0);
        assert_eq!(ring.spread, 1.0);
        // The render grow term is `blur + spread` = 1.0 — a 1-DIP outline.
        assert_eq!(ring.blur.max(0.0) + ring.spread.max(0.0), 1.0);
    }

    #[test]
    fn neo_dual_has_negative_offset_light_extrude() {
        // neo: +offset dark + −offset light — the −6,−6 layer shifts up-left.
        let dark = Shadow::drop(6.0, 6.0, 12.0, Color::from_u8(0xA3, 0xB1, 0xC6, 0x99));
        let light = Shadow::drop(-6.0, -6.0, 12.0, Color::from_u8(0xFF, 0xFF, 0xFF, 0xCC));
        let s = ShadowStack::two(dark, light);
        assert_eq!(s.layers()[1].offset_x, -6.0);
        assert_eq!(s.layers()[1].offset_y, -6.0);
    }
}
