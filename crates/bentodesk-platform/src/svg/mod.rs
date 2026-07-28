//! SVG → D2D `ID2D1PathGeometry` converter (Lucide-icon coverage).
//!
//! # Why hand-rolled
//!
//! Per spec §8.1 the `lunasvg-rs` / `usvg` / `resvg` / `image` family of crates
//! is permanently banned. This is the workspace's only SVG parser. The parent
//! BentoDesk app uses Lucide-style line icons (`lucide-static@0.471.0`) plus
//! a handful of custom decorations — all single-color stroked paths with
//! occasional `<circle>` shapes and a small subset of SVG path commands. No
//! Lucide icon uses `<defs>` / `<use>` / `<linearGradient>` / nested `<g
//! transform>` in practice; we still parse those constructs so future custom
//! icons that exercise them do not silently render blank.
//!
//! # Pipeline
//!
//! 1. [`Parsed::from_bytes`] — one-shot tokenize + element walk. Allowed
//!    to allocate (icon-load path, not paint-time).
//! 2. [`Parsed::to_d2d_geometry`] — build `ID2D1PathGeometry` once per icon.
//!    Cache via [`crate::svg_cache::SvgCache`].
//! 3. Paint reuses the geometry — zero allocation per frame (spec §10).
//!
//! # Spec compliance
//!
//! - §8 / §8.1 — zero new dependencies. Tokenizer + LRU + hash all hand-rolled.
//! - §10 — `Parsed::to_d2d_geometry` is one-shot per icon; cache hits skip it.
//! - §11 — every fallible call returns [`PlatformError::Svg`]; no `unwrap` /
//!   `expect` / `panic!`.
//! - §11.1 — every `unsafe` block carries a `// SAFETY:` comment.
//! - §15 — every submodule below is ≤ 800 LOC.

mod d2d;
mod parser;
mod path_d;
mod shapes;
mod types;
mod util;

use smallvec::SmallVec;
use std::collections::HashMap;
use windows::Win32::Graphics::Direct2D::{ID2D1Factory1, ID2D1PathGeometry};

use crate::errors::PlatformError;

pub use types::{
    Affine, Cmd, DefinedElement, GradientStop, LinearGradient, Parsed, ParsedPath, ViewBox,
};

impl Parsed {
    /// Tokenize + walk an SVG document. One-shot; caller is expected to cache
    /// the result (see [`crate::svg_cache`]).
    pub fn from_bytes(input: &[u8]) -> Result<Self, PlatformError> {
        let src = std::str::from_utf8(input)
            .map_err(|_| PlatformError::Svg("svg input is not valid utf-8"))?;
        let mut parser = parser::Parser::new(src);
        parser.parse_document()
    }
}

// ---------------------------------------------------------------------------
// Back-compat shim — Wave-19 callers built `ID2D1PathGeometry` straight from a
// raw `<path d="...">` string. Keep that surface working until they migrate to
// `Parsed::from_bytes(...)`.
// ---------------------------------------------------------------------------

/// Lucide-style `home` icon 24×24, straight-line subset.
pub const HOME_PATH: &str = "M3 9 L12 2 L21 9 L21 22 L15 22 L15 14 L10 14 L10 22 L3 22 Z";

/// Build a single-path geometry from a bare SVG `d` attribute. Used by widgets
/// that only need the legacy straight-line subset; new code paths should use
/// [`Parsed::from_bytes`] + [`Parsed::to_d2d_geometry`] for full SVG support.
pub fn build(factory: &ID2D1Factory1, path_d: &str) -> Result<ID2D1PathGeometry, PlatformError> {
    let mut path = ParsedPath::default();
    path_d::parse_path_d(path_d.as_bytes(), &mut path, Affine::IDENTITY)?;
    let parsed = Parsed {
        paths: {
            let mut v: SmallVec<[ParsedPath; 4]> = SmallVec::new();
            v.push(path);
            v
        },
        viewbox: ViewBox::default(),
        defs: HashMap::new(),
        gradients: HashMap::new(),
    };
    parsed.to_d2d_geometry(factory)
}

// ---------------------------------------------------------------------------
// Tests — exercises the full parse pipeline against real Lucide samples plus
// synthetic SVGs covering each construct.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::util::read_num;

    const ACTIVITY: &[u8] = br#"<svg viewBox="0 0 24 24"><path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/></svg>"#;
    const ANCHOR: &[u8] = br#"<svg viewBox="0 0 24 24"><path d="M12 22V8"/><path d="M5 12H2a10 10 0 0 0 20 0h-3"/><circle cx="12" cy="5" r="3"/></svg>"#;
    const APERTURE: &[u8] = br#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="m14.31 8 5.74 9.94"/><path d="M9.69 8h11.48"/><path d="m7.38 12 5.74-9.94"/><path d="M9.69 16 3.95 6.06"/><path d="M14.31 16H2.83"/><path d="m16.62 12-5.74 9.94"/></svg>"#;
    const SETTINGS: &[u8] = br#"<svg viewBox="0 0 24 24"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>"#;
    const LOADER_CIRCLE: &[u8] =
        br#"<svg viewBox="0 0 24 24"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>"#;

    fn parse_or_skip(name: &str, bytes: &[u8]) -> Option<Parsed> {
        let res = Parsed::from_bytes(bytes);
        assert!(res.is_ok(), "{name} parse failed: {:?}", res.as_ref().err());
        res.ok()
    }

    #[test]
    fn lucide_activity_one_path_with_arcs() {
        let Some(p) = parse_or_skip("activity", ACTIVITY) else {
            return;
        };
        assert_eq!(p.paths.len(), 1);
        assert_eq!(
            p.viewbox,
            ViewBox {
                min_x: 0.0,
                min_y: 0.0,
                width: 24.0,
                height: 24.0
            }
        );
        assert!(
            p.paths[0]
                .commands
                .iter()
                .any(|c| matches!(c, Cmd::Cubic { .. })),
            "arcs should lower to cubics"
        );
    }

    #[test]
    fn lucide_anchor_two_paths_one_circle() {
        let Some(p) = parse_or_skip("anchor", ANCHOR) else {
            return;
        };
        assert_eq!(p.paths.len(), 3, "two <path> + one <circle>");
        let circle = &p.paths[2];
        assert!(matches!(circle.commands.first(), Some(Cmd::Move(_, _))));
        assert!(
            circle
                .commands
                .iter()
                .filter(|c| matches!(c, Cmd::Cubic { .. }))
                .count()
                >= 4
        );
        assert!(matches!(circle.commands.last(), Some(Cmd::Close)));
    }

    #[test]
    fn lucide_aperture_six_paths_one_circle() {
        let Some(p) = parse_or_skip("aperture", APERTURE) else {
            return;
        };
        assert_eq!(p.paths.len(), 7);
    }

    #[test]
    fn lucide_settings_path_plus_circle() {
        let Some(p) = parse_or_skip("settings", SETTINGS) else {
            return;
        };
        assert_eq!(p.paths.len(), 2);
    }

    #[test]
    fn lucide_loader_circle_arc_lowering() {
        let Some(p) = parse_or_skip("loader-circle", LOADER_CIRCLE) else {
            return;
        };
        assert_eq!(p.paths.len(), 1);
        let cubics = p.paths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, Cmd::Cubic { .. }))
            .count();
        assert!(cubics >= 1, "the single arc should lower to ≥ 1 cubic");
    }

    #[test]
    fn defs_use_resolves_with_translate() {
        let svg = br##"<svg viewBox="0 0 24 24">
            <defs><path id="a" d="M0 0 L10 0 L10 10 Z"/></defs>
            <use href="#a" x="5" y="5"/>
        </svg>"##;
        let Some(p) = parse_or_skip("defs/use", svg) else {
            return;
        };
        assert_eq!(p.paths.len(), 1);
        let first = p.paths[0].commands.first();
        let is_translated = matches!(
            first,
            Some(Cmd::Move(x, y)) if (*x - 5.0).abs() < 1e-4 && (*y - 5.0).abs() < 1e-4
        );
        assert!(
            is_translated,
            "expected Move at translated origin, got {first:?}"
        );
    }

    #[test]
    fn group_transform_propagates() {
        let svg = br#"<svg viewBox="0 0 24 24">
            <g transform="translate(10 0)"><path d="M0 0 L5 0"/></g>
        </svg>"#;
        let Some(p) = parse_or_skip("<g transform>", svg) else {
            return;
        };
        assert_eq!(p.paths.len(), 1);
        let first = p.paths[0].commands.first();
        let is_translated = matches!(first, Some(Cmd::Move(x, _)) if (*x - 10.0).abs() < 1e-4);
        assert!(is_translated, "expected translated Move, got {first:?}");
    }

    #[test]
    fn linear_gradient_stops_captured() {
        let svg = br##"<svg viewBox="0 0 24 24">
            <defs>
                <linearGradient id="g">
                    <stop offset="0%" stop-color="#ff0000"/>
                    <stop offset="100%" stop-color="#0000ff" stop-opacity="0.5"/>
                </linearGradient>
            </defs>
        </svg>"##;
        let Some(p) = parse_or_skip("linearGradient", svg) else {
            return;
        };
        let g_opt = p.gradients.get("g");
        assert!(g_opt.is_some(), "gradient 'g' not registered");
        let Some(g) = g_opt else { return };
        assert_eq!(g.stops.len(), 2);
        assert_eq!(g.stops[0].rgba, [0xFF, 0x00, 0x00, 0xFF]);
        assert_eq!(g.stops[1].rgba[3], (0.5 * 255.0) as u8);
    }

    #[test]
    fn smooth_cubic_reflects_previous_control() {
        let svg = br#"<svg viewBox="0 0 24 24"><path d="M0 0 C 0 5 5 5 5 0 S 10 -5 10 0"/></svg>"#;
        let Some(p) = parse_or_skip("S command", svg) else {
            return;
        };
        let cubics = p.paths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, Cmd::Cubic { .. }))
            .count();
        assert_eq!(cubics, 2);
    }

    #[test]
    fn quadratic_and_smooth_quadratic_supported() {
        let svg = br#"<svg viewBox="0 0 24 24"><path d="M0 0 Q 5 5 10 0 T 20 0"/></svg>"#;
        let Some(p) = parse_or_skip("Q + T", svg) else {
            return;
        };
        let quads = p.paths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, Cmd::Quad { .. }))
            .count();
        assert_eq!(quads, 2);
    }

    #[test]
    fn implicit_repeat_after_move_treats_as_lineto() {
        let svg = br#"<svg viewBox="0 0 24 24"><path d="M1 1 2 2 3 3"/></svg>"#;
        let Some(p) = parse_or_skip("implicit lineto", svg) else {
            return;
        };
        let lines = p.paths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, Cmd::Line(_, _)))
            .count();
        assert_eq!(lines, 2);
    }

    #[test]
    fn read_num_handles_negative_and_decimal() {
        let res = read_num(b"-2.5 ", 0);
        assert!(
            res.is_ok(),
            "read_num must accept '-2.5'; got {:?}",
            res.as_ref().err()
        );
        if let Ok((v, _)) = res {
            assert!((v + 2.5).abs() < 1e-5);
        }
    }

    #[test]
    fn lucide_home_uses_supported_commands_only() {
        for c in HOME_PATH.bytes() {
            if c.is_ascii_alphabetic() {
                assert!(
                    matches!(
                        c,
                        b'M' | b'm' | b'L' | b'l' | b'H' | b'h' | b'V' | b'v' | b'Z' | b'z'
                    ),
                    "unexpected command in HOME_PATH: {}",
                    c as char
                );
            }
        }
    }

    #[test]
    fn estimated_bytes_scales_with_command_count() {
        let Some(small) = parse_or_skip("loader-circle", LOADER_CIRCLE) else {
            return;
        };
        let Some(big) = parse_or_skip("settings", SETTINGS) else {
            return;
        };
        assert!(big.estimated_bytes() > small.estimated_bytes());
    }
}
