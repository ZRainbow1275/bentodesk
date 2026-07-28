//! Pure data types shared across the SVG parser submodules.
//!
//! All resolved-coordinate (post-transform) — D2D consumers see absolute
//! user-space numbers without further state.

use smallvec::SmallVec;
use smol_str::SmolStr;
use std::collections::HashMap;

/// Axis-aligned rectangle in user-space units (SVG `viewBox`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBox {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for ViewBox {
    fn default() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            width: 24.0,
            height: 24.0,
        }
    }
}

/// Row-major 3×2 affine. `[a b c d e f]` per the SVG `transform` matrix form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Compose `self * rhs` so that `self.apply(rhs.apply(p)) == compose.apply(p)`.
    #[must_use]
    pub fn compose(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    #[must_use]
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub(crate) fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }
}

/// One geometry-sink command in user space (already with group transform baked
/// in). All deltas resolved to absolute coordinates so the D2D builder is
/// stateless.
#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    Cubic {
        c1x: f32,
        c1y: f32,
        c2x: f32,
        c2y: f32,
        x: f32,
        y: f32,
    },
    Quad {
        c1x: f32,
        c1y: f32,
        x: f32,
        y: f32,
    },
    Close,
}

/// One drawable path. The `commands` stream is resolved to absolute user-space
/// coordinates so the D2D builder can emit calls without further state.
#[derive(Clone, Debug, Default)]
pub struct ParsedPath {
    pub commands: SmallVec<[Cmd; 16]>,
}

/// Element entry stored in the `<defs>` table — currently `<path>` (and any
/// shape lowered into one) only; gradients live in the `gradients` map.
#[derive(Clone, Debug)]
pub enum DefinedElement {
    Path(ParsedPath),
}

/// Linear gradient definition. Captured for forward-compat with future custom
/// icons; Lucide itself does not use gradients.
#[derive(Clone, Debug)]
pub struct LinearGradient {
    pub stops: SmallVec<[GradientStop; 4]>,
    pub transform: Affine,
}

#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    pub offset: f32,
    pub rgba: [u8; 4],
}

/// Result of [`super::Parsed::from_bytes`] — fully resolved icon ready for D2D
/// geometry construction.
#[derive(Clone, Debug)]
pub struct Parsed {
    pub paths: SmallVec<[ParsedPath; 4]>,
    pub viewbox: ViewBox,
    pub defs: HashMap<SmolStr, DefinedElement>,
    pub gradients: HashMap<SmolStr, LinearGradient>,
}

impl Parsed {
    /// Approximate byte cost of holding this `Parsed` + its eventual D2D
    /// geometry in cache. Used by [`crate::svg_cache::SvgCache`] for the
    /// 8 MB ceiling enforcement.
    #[must_use]
    pub fn estimated_bytes(&self) -> usize {
        let cmd_size = core::mem::size_of::<Cmd>();
        let cmd_count: usize = self.paths.iter().map(|p| p.commands.len()).sum();
        let geom_overhead = self.paths.len().saturating_mul(1024);
        cmd_count
            .saturating_mul(cmd_size)
            .saturating_add(geom_overhead)
    }
}
