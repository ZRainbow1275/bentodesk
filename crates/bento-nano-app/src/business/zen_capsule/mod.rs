//! Business surface — `ZenCapsule`, the collapsed pill state of a BentoZone.
//!
//! Visual spec: see `zen_capsule.snap.md`. Three size / three shape variants
//! are locked here as the wire-format contract; downstream layout JSON
//! (`zones[i].appearance`) round-trips through the `CapsuleShape` /
//! `CapsuleSize` enums via serde.
//!
//! Status: scaffolding per Wave E Option-A (snap.md + compile-clean
//! Container + locked helpers/wire-format). The composition body — icon +
//! title + badge children — lands when widget-library ships the SvgIcon /
//! Text primitives we depend on. NOT a `todo!()` stub.

use bento_nano_layout::Direction;
use bento_nano_style::{Edges, Length};
use bento_nano_widget::{ContainerNode, WidgetNode};
use serde::{Deserialize, Serialize};

/// Capsule outline shape — locked wire-format. Defaults to `Pill` to match
/// the 1.x `bentoZone.appearance.capsule_shape` default.
///
/// M2② (2026-05-29) — reconciled to Tauri v1.3.0's FOUR pill shapes
/// (`BentoZone.css:80-99` / `ZenCapsule.css:47-76`): `pill` / `rounded` /
/// `circle` / `minimal`. The original three (`Pill` / `Rounded` / `Square`)
/// are PRESERVED IN ORDER so saved `zones.bin` keeps deserializing — the two
/// Tauri-only shapes are *appended* (`Circle`, `Minimal`), never inserted, and
/// no existing variant is renamed/reordered (wire-format safety). `Square` is
/// retained as a legacy back-compat variant: 1.x layout JSON written before
/// the Tauri reconciliation may still carry `"square"`, which renders as a
/// near-sharp boxy capsule. The editor (`zone_editor::CapsuleShapeChoice`)
/// already exposes the Tauri-canonical four (pill/rounded/circle/minimal); a
/// freshly-picked shape therefore never persists `"square"` going forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleShape {
    #[default]
    Pill,
    Rounded,
    Square,
    // --- Appended for Tauri parity (additive, wire-safe) ---
    Circle,
    Minimal,
}

impl CapsuleShape {
    /// Parse the lowercase wire token written to `Zone.capsule_shape`. Unknown
    /// tokens fall back to [`Self::default`] (`Pill`) so a forward-compat
    /// layout JSON never bricks the live render. Cheap `match` on `&str`
    /// (spec §10 hot-path safe — no alloc, no `format!`).
    pub fn parse(token: &str) -> Self {
        match token {
            "pill" => Self::Pill,
            "rounded" => Self::Rounded,
            "square" => Self::Square,
            "circle" => Self::Circle,
            "minimal" => Self::Minimal,
            _ => Self::default(),
        }
    }

    /// Whether this shape is rendered as a 1:1 (square-aspect) capsule. Tauri's
    /// `.zen-capsule--circle` sets `aspect-ratio: 1` + `border-radius: 50%`
    /// (`ZenCapsule.css:55-61`), collapsing the pill to an icon-only disc.
    #[inline]
    pub const fn is_circle(self) -> bool {
        matches!(self, Self::Circle)
    }

    /// Corner radius in logical pixels for a capsule of outer `height_px`,
    /// matching Tauri's per-shape `border-radius` (`BentoZone.css:80-99`):
    ///
    /// * `pill` → `24px` (Tauri fixed `--shape-pill`; equals `height/2` at the
    ///   Medium 48px tier so the capsule reads as a true stadium).
    /// * `rounded` → `12px` (`--shape-rounded`).
    /// * `circle` → `height/2` (Tauri `50%` on a 1:1 box = a perfect disc).
    /// * `minimal` → `8px` (`--shape-minimal`).
    /// * `square` → `4px` (legacy boxy capsule; Tauri has no `square`, so this
    ///   is nano's own near-sharp treatment retained for back-compat).
    ///
    /// Stack-only `match` on a `Copy` enum (spec §10) — no alloc, no panic.
    #[inline]
    pub fn corner_radius_px(self, height_px: f32) -> f32 {
        match self {
            Self::Pill => 24.0,
            Self::Rounded => 12.0,
            Self::Circle => height_px * 0.5,
            Self::Minimal => 8.0,
            Self::Square => 4.0,
        }
    }
}

/// Capsule size — locked wire-format. Defaults to `Medium`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CapsuleSize {
    /// Capsule outer height in logical pixels — snap.md mandated.
    ///
    /// M2② (2026-05-29) — re-centred on Tauri v1.3.0's `getCapsuleBoxPx`
    /// (`bentodesk/src/services/hitTest.ts:94-99`), the authoritative pixel
    /// source for the collapsed capsule box: `small {height:36}` /
    /// `medium {height:48}` / `large {height:56}`. Medium (the default tier)
    /// grows 44→48 to match Tauri 1:1 (Q2 pixel parity); Large 52→56. Small
    /// already matched at 36. snap.md updated in lockstep.
    pub const fn height_px(self) -> f32 {
        match self {
            Self::Small => 36.0,
            Self::Medium => 48.0,
            Self::Large => 56.0,
        }
    }

    /// Default capsule outer width in logical pixels for the non-circle tiers,
    /// from Tauri `getCapsuleBoxPx` (`hitTest.ts:94-99`): `small {width:120}` /
    /// `medium {width:160}` / `large {width:200}`. The live nano pill sizes its
    /// width dynamically from the label + badge run (see
    /// `zone_pill_geometry::pill_layout_for_zone`), so this is the *circle*
    /// 1:1 fallback / reference width rather than a hard clamp.
    pub const fn width_px(self) -> f32 {
        match self {
            Self::Small => 120.0,
            Self::Medium => 160.0,
            Self::Large => 200.0,
        }
    }

    /// Diameter in logical pixels when the shape is `Circle` (1:1 aspect), from
    /// Tauri `getCapsuleBoxPx` (`hitTest.ts:89-92`): `small 42 / medium 52 /
    /// large 64`. A circle pill is square so width == height == this value.
    pub const fn circle_diameter_px(self) -> f32 {
        match self {
            Self::Small => 42.0,
            Self::Medium => 52.0,
            Self::Large => 64.0,
        }
    }

    /// Icon size in logical pixels.
    ///
    /// M2② (2026-05-29) — aligned to Tauri's `.zen-capsule__icon` font-size
    /// per size tier (`ZenCapsule.css`): small `14px` (`:85`), medium `18px`
    /// (base `:14`), large `22px` (`:105`). Replaces the pre-Tauri 20/24/28
    /// stand-ins. snap.md updated in lockstep.
    pub const fn icon_px(self) -> f32 {
        match self {
            Self::Small => 14.0,
            Self::Medium => 18.0,
            Self::Large => 22.0,
        }
    }

    /// Parse the lowercase wire token written to `Zone.capsule_size`. Unknown
    /// tokens fall back to [`Self::default`] (`Medium`). Cheap `match` on
    /// `&str` (spec §10 hot-path safe).
    pub fn parse(token: &str) -> Self {
        match token {
            "small" => Self::Small,
            "medium" => Self::Medium,
            "large" => Self::Large,
            _ => Self::default(),
        }
    }
}

/// Build the collapsed-pill subtree for a BentoZone with default
/// (`Pill` / `Medium`) appearance. The variant-aware constructor below is
/// what the panel layer will call once the widget primitives ship.
pub fn build() -> WidgetNode {
    build_with(CapsuleShape::default(), CapsuleSize::default())
}

/// Build the collapsed-pill subtree with explicit shape + size.
/// Geometry is locked per `zen_capsule.snap.md`; only the inner composition
/// (icon + title + badge children) waits on widget-library primitives.
pub fn build_with(_shape: CapsuleShape, size: CapsuleSize) -> WidgetNode {
    WidgetNode::Container(ContainerNode {
        direction: Direction::Row,
        width: Length::Auto,
        height: Length::Px(size.height_px()),
        padding: Edges {
            top: 6.0,
            right: 14.0,
            bottom: 6.0,
            left: 14.0,
        },
        ..ContainerNode::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_nano_layout::LayoutSource;

    #[test]
    fn defaults_match_snap_md() {
        assert_eq!(CapsuleShape::default(), CapsuleShape::Pill);
        assert_eq!(CapsuleSize::default(), CapsuleSize::Medium);
    }

    #[test]
    fn size_height_table_matches_tauri() {
        // M2② — Tauri `getCapsuleBoxPx` heights (hitTest.ts:94-99).
        assert!((CapsuleSize::Small.height_px() - 36.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.height_px() - 48.0).abs() < 0.01);
        assert!((CapsuleSize::Large.height_px() - 56.0).abs() < 0.01);
    }

    #[test]
    fn size_icon_table_matches_tauri() {
        // M2② — Tauri `.zen-capsule__icon` per-tier font-size (ZenCapsule.css).
        assert!((CapsuleSize::Small.icon_px() - 14.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.icon_px() - 18.0).abs() < 0.01);
        assert!((CapsuleSize::Large.icon_px() - 22.0).abs() < 0.01);
    }

    #[test]
    fn circle_diameter_table_matches_tauri() {
        // M2② — Tauri `getCapsuleBoxPx` circle branch (hitTest.ts:89-92).
        assert!((CapsuleSize::Small.circle_diameter_px() - 42.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.circle_diameter_px() - 52.0).abs() < 0.01);
        assert!((CapsuleSize::Large.circle_diameter_px() - 64.0).abs() < 0.01);
    }

    #[test]
    fn shape_corner_radius_matches_tauri() {
        // M2② — per-shape border-radius (BentoZone.css:80-99). Pill/rounded/
        // minimal are Tauri fixed; circle is height/2 (50% on a 1:1 box).
        let h = CapsuleSize::Medium.height_px(); // 48
        assert!((CapsuleShape::Pill.corner_radius_px(h) - 24.0).abs() < 0.01);
        assert!((CapsuleShape::Rounded.corner_radius_px(h) - 12.0).abs() < 0.01);
        assert!((CapsuleShape::Minimal.corner_radius_px(h) - 8.0).abs() < 0.01);
        assert!((CapsuleShape::Circle.corner_radius_px(h) - 24.0).abs() < 0.01);
        assert!((CapsuleShape::Square.corner_radius_px(h) - 4.0).abs() < 0.01);
    }

    #[test]
    fn shape_and_size_parse_round_trip_tokens() {
        assert_eq!(CapsuleShape::parse("pill"), CapsuleShape::Pill);
        assert_eq!(CapsuleShape::parse("rounded"), CapsuleShape::Rounded);
        assert_eq!(CapsuleShape::parse("square"), CapsuleShape::Square);
        assert_eq!(CapsuleShape::parse("circle"), CapsuleShape::Circle);
        assert_eq!(CapsuleShape::parse("minimal"), CapsuleShape::Minimal);
        // Unknown → default.
        assert_eq!(CapsuleShape::parse("hexagon"), CapsuleShape::Pill);
        assert_eq!(CapsuleSize::parse("small"), CapsuleSize::Small);
        assert_eq!(CapsuleSize::parse("medium"), CapsuleSize::Medium);
        assert_eq!(CapsuleSize::parse("large"), CapsuleSize::Large);
        assert_eq!(CapsuleSize::parse("xl"), CapsuleSize::Medium);
    }

    #[test]
    fn build_uses_medium_height() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Row);
        assert!(matches!(layout.height, Length::Px(h) if (h - 48.0).abs() < 0.01));
    }

    #[test]
    fn build_with_size_overrides_height() {
        let node = build_with(CapsuleShape::Rounded, CapsuleSize::Large);
        let layout = node.layout();
        assert!(matches!(layout.height, Length::Px(h) if (h - 56.0).abs() < 0.01));
    }

    /// Wire-format lock: 1.x `appearance.capsule_shape = "pill"` + serde must
    /// continue to deserialize after the Rust rewrite. M2② extends the set to
    /// the Tauri-parity four (pill/rounded/circle/minimal) plus the legacy
    /// `square` back-compat variant — ALL five must round-trip, and the two
    /// pre-existing back-compat tags (`square`) must still deserialize so
    /// saved `zones.bin`/layout JSON keeps loading.
    #[test]
    fn capsule_shape_serde_round_trip() {
        for v in [
            CapsuleShape::Pill,
            CapsuleShape::Rounded,
            CapsuleShape::Square,
            CapsuleShape::Circle,
            CapsuleShape::Minimal,
        ] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: CapsuleShape = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        // Canonical wire tags.
        assert_eq!(
            serde_json::to_string(&CapsuleShape::Pill).unwrap_or_default(),
            "\"pill\""
        );
        assert_eq!(
            serde_json::to_string(&CapsuleShape::Circle).unwrap_or_default(),
            "\"circle\""
        );
        assert_eq!(
            serde_json::to_string(&CapsuleShape::Minimal).unwrap_or_default(),
            "\"minimal\""
        );
        // Back-compat: the legacy `"square"` tag from pre-Tauri saves must
        // still deserialize to the retained `Square` variant.
        let legacy: CapsuleShape =
            serde_json::from_str("\"square\"").unwrap_or_default();
        assert_eq!(legacy, CapsuleShape::Square);
        // Forward-compat: every canonical token deserializes.
        for (tok, want) in [
            ("\"pill\"", CapsuleShape::Pill),
            ("\"rounded\"", CapsuleShape::Rounded),
            ("\"circle\"", CapsuleShape::Circle),
            ("\"minimal\"", CapsuleShape::Minimal),
        ] {
            let got: CapsuleShape = serde_json::from_str(tok).unwrap_or_default();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn capsule_size_serde_round_trip() {
        for v in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: CapsuleSize = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        assert_eq!(
            serde_json::to_string(&CapsuleSize::Medium).unwrap_or_default(),
            "\"medium\""
        );
    }
}
