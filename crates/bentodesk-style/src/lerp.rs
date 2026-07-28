//! `Lerp` — linear interpolation primitive + impls for style types.
//!
//! Per the team-lead's §13 ruling (orphan-rule resolution for the
//! animation-crate split), the `Lerp` trait now lives in `bentodesk-style`
//! alongside the concrete types it operates on. `bentodesk-tree::animation`
//! re-exports it for backward compatibility — existing call sites that
//! import `bentodesk_tree::Lerp` continue to compile unchanged.
//!
//! All impls are `Copy`-respecting and branch-light. The per-frame tween
//! path flows through these without allocating. Colour interpolation uses
//! premultiplied alpha so a fade across alpha doesn't leak the source colour
//! through transparent intermediate frames.

use crate::{Color, Length, Rect, Size};

/// Linear interpolation contract. `t` is clamped to `0.0..=1.0` by each impl
/// so accumulated-time drivers can drift slightly past the endpoint without
/// over/undershoot feeding back into the tween.
pub trait Lerp: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        // Mul-add keeps the FMA-friendly form on modern CPUs and avoids the
        // catastrophic cancellation of `self + (other - self) * t` near edges.
        self * (1.0 - t) + other * t
    }
}

impl Lerp for Color {
    /// Premultiplied-alpha lerp (matches CSS / Skia / DirectComposition
    /// blending semantics). Naïve per-channel lerp + per-channel alpha lerp
    /// produces a banding artefact when one endpoint is transparent: midway
    /// through, the fully-saturated rgb of the opaque endpoint shows through
    /// the partial alpha. Premultiplying first, lerping the four channels,
    /// then un-premultiplying yields the perceptually-correct fade.
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;

        // Pre-multiply each endpoint.
        let ar = self.r * self.a;
        let ag = self.g * self.a;
        let ab = self.b * self.a;
        let br = other.r * other.a;
        let bg = other.g * other.a;
        let bb = other.b * other.a;

        let a = self.a * inv + other.a * t;
        let pr = ar * inv + br * t;
        let pg = ag * inv + bg * t;
        let pb = ab * inv + bb * t;

        // Un-premultiply, guarding against `a == 0` (fully transparent — the
        // rgb carries no perceptual signal anyway, return zeros).
        if a < f32::EPSILON {
            Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
        } else {
            Color {
                r: pr / a,
                g: pg / a,
                b: pb / a,
                a,
            }
        }
    }
}

impl Lerp for Rect {
    /// Component-wise lerp on `(x, y, width, height)`.
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        Rect {
            x: self.x * inv + other.x * t,
            y: self.y * inv + other.y * t,
            width: self.width * inv + other.width * t,
            height: self.height * inv + other.height * t,
        }
    }
}

impl Lerp for Size {
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv = 1.0 - t;
        Size {
            width: self.width * inv + other.width * t,
            height: self.height * inv + other.height * t,
        }
    }
}

impl Lerp for Length {
    /// `Length` is an enum mixing `Px(f32)`, `Fraction(f32)`, and `Auto`.
    /// Same-variant lerp is straightforward; cross-variant lerp falls back to
    /// the destination value at/past the midpoint to avoid undefined
    /// arithmetic across `Auto`.
    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        match (self, other) {
            (Length::Px(a), Length::Px(b)) => Length::Px(a * (1.0 - t) + b * t),
            (Length::Fraction(a), Length::Fraction(b)) => Length::Fraction(a * (1.0 - t) + b * t),
            // Cross-variant or Auto — switch to the destination once t passes
            // the midpoint. Avoids producing a `Length::Auto` interpolated
            // with a numeric variant which has no sensible meaning.
            (_, dst) if t >= 0.5 => dst,
            (src, _) => src,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn lerp_f32_midpoint() {
        assert!(close(f32::lerp(0.0, 10.0, 0.5), 5.0));
        assert!(close(f32::lerp(2.0, 6.0, 0.0), 2.0));
        assert!(close(f32::lerp(2.0, 6.0, 1.0), 6.0));
    }

    #[test]
    fn color_lerp_endpoints_are_exact() {
        let a = Color::from_u8(0x10, 0x20, 0x30, 0xFF);
        let b = Color::from_u8(0xC0, 0xD0, 0xE0, 0xFF);
        let l0 = a.lerp(b, 0.0);
        let l1 = a.lerp(b, 1.0);
        assert!(close(l0.r, a.r) && close(l0.g, a.g) && close(l0.b, a.b) && close(l0.a, a.a));
        assert!(close(l1.r, b.r) && close(l1.g, b.g) && close(l1.b, b.b) && close(l1.a, b.a));
    }

    #[test]
    fn color_lerp_to_transparent_does_not_band() {
        // Opaque red → fully transparent. The midpoint must remain red-ish
        // (premultiplied lerp preserves identity) instead of grey.
        let opaque_red = Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let xp = Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        let mid = opaque_red.lerp(xp, 0.5);
        assert!(
            close(mid.r, 1.0),
            "midpoint red should stay 1.0, got {}",
            mid.r
        );
        assert!(close(mid.g, 0.0));
        assert!(close(mid.b, 0.0));
        assert!(close(mid.a, 0.5));
    }

    #[test]
    fn color_lerp_to_zero_alpha_returns_zero_rgb() {
        // Both endpoints fully transparent — guard against div-by-zero.
        let a = Color {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 0.0,
        };
        let b = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        let mid = a.lerp(b, 0.5);
        assert!(close(mid.a, 0.0));
        assert!(close(mid.r, 0.0));
    }

    #[test]
    fn color_lerp_clamps_out_of_range_t() {
        let a = Color::from_u8(0x00, 0x00, 0x00, 0xFF);
        let b = Color::from_u8(0xFF, 0xFF, 0xFF, 0xFF);
        // Negative t must pin to source.
        assert_eq!(a.lerp(b, -0.5), a);
        // t > 1 must pin to destination.
        assert_eq!(a.lerp(b, 1.5), b);
    }

    #[test]
    fn rect_lerp_is_componentwise() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let b = Rect {
            x: 200.0,
            y: 100.0,
            width: 300.0,
            height: 150.0,
        };
        let mid = a.lerp(b, 0.5);
        assert!(close(mid.x, 100.0));
        assert!(close(mid.y, 50.0));
        assert!(close(mid.width, 200.0));
        assert!(close(mid.height, 100.0));
    }

    #[test]
    fn size_lerp_endpoints_match() {
        let a = Size {
            width: 10.0,
            height: 20.0,
        };
        let b = Size {
            width: 30.0,
            height: 40.0,
        };
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
    }

    #[test]
    fn length_lerp_same_variant_interpolates_numerically() {
        let a = Length::Px(0.0);
        let b = Length::Px(100.0);
        assert!(matches!(a.lerp(b, 0.25), Length::Px(v) if close(v, 25.0)));

        let a = Length::Fraction(0.0);
        let b = Length::Fraction(1.0);
        assert!(matches!(
            a.lerp(b, 0.5),
            Length::Fraction(v) if close(v, 0.5)
        ));
    }

    #[test]
    fn length_lerp_cross_variant_switches_at_midpoint() {
        let a = Length::Auto;
        let b = Length::Px(50.0);
        // Below midpoint — keep source.
        assert!(matches!(a.lerp(b, 0.49), Length::Auto));
        // At/past midpoint — switch to destination.
        assert!(matches!(a.lerp(b, 0.5), Length::Px(_)));
    }
}
