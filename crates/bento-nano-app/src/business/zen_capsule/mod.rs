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

    /// G5 (2026-06-01) — asymmetric horizontal padding `(pad_left, pad_right)`
    /// in logical pixels, matching Tauri's `.zen-capsule` per-tier padding
    /// (`ZenCapsule.css:8` base `0 var(--spacing-lg) 0 var(--spacing-xl)` =
    /// right 16 / left 20 at medium; `:81` small `0 12 0 12`; `:101` large
    /// `0 20 0 28`). The pre-G5 geometry used a single symmetric `SPACING.md`
    /// (12) on every tier, so medium/large read tighter and not left-weighted.
    ///
    /// * Small → `(12, 12)` (`--spacing-md` both sides).
    /// * Medium → `(20, 16)` (`--spacing-xl` left / `--spacing-lg` right).
    /// * Large → `(28, 20)` (`--spacing-2xl` left / `--spacing-xl` right).
    ///
    /// Stack-only `match` on a `Copy` enum (spec §10) — no alloc, no panic.
    #[inline]
    pub const fn pad_lr_px(self) -> (f32, f32) {
        match self {
            Self::Small => (12.0, 12.0),
            Self::Medium => (20.0, 16.0),
            Self::Large => (28.0, 20.0),
        }
    }

    /// G5 (2026-06-01) — inner gap between icon / title / badge in logical
    /// pixels, matching Tauri's single `.zen-capsule { gap }` per-tier
    /// (`ZenCapsule.css:9` base `var(--spacing-md)`=12; `:82` small
    /// `--spacing-sm`=8; `:102` large `--spacing-lg`=16). The pre-G5 geometry
    /// used a flat `SPACING.s6` (6) for all tiers (medium read at half the
    /// Tauri gap). Tauri uses the SAME gap both icon→title and title→badge.
    #[inline]
    pub const fn inner_gap_px(self) -> f32 {
        match self {
            Self::Small => 8.0,
            Self::Medium => 12.0,
            Self::Large => 16.0,
        }
    }

    /// G5 (2026-06-01) — title font size in logical pixels per tier, matching
    /// Tauri's `.zen-capsule__title` font-size (`ZenCapsule.css:25` medium
    /// `--font-size-md`=14; `:90` small `--font-size-xs`=11; `:110` large
    /// `--font-size-lg`=16). The pre-G5 renderer drew the label with the global
    /// default 16px format on every tier (small over-sized, large under-sized).
    #[inline]
    pub const fn title_font_px(self) -> f32 {
        match self {
            Self::Small => 11.0,
            Self::Medium => 14.0,
            Self::Large => 16.0,
        }
    }

    /// G5 (2026-06-01) — count-badge font size in logical pixels per tier,
    /// matching Tauri's `.zen-capsule__badge` font-size (`ZenCapsule.css:35`
    /// medium `--font-size-xs`=11; `:94` small `10`; `:114` large
    /// `--font-size-sm`=13). Drawn at `--font-weight-semibold`=600
    /// ([`badge_font_weight`]).
    #[inline]
    pub const fn badge_font_px(self) -> f32 {
        match self {
            Self::Small => 10.0,
            Self::Medium => 11.0,
            Self::Large => 13.0,
        }
    }

    /// G5 (2026-06-01) — count-badge weight. Tauri `.zen-capsule__badge` is
    /// `--font-weight-semibold`=600 on every tier (`ZenCapsule.css:36`); the
    /// pre-G5 renderer drew it at the default medium (500) body weight.
    #[inline]
    pub const fn badge_font_weight(self) -> u16 {
        600
    }

    /// G5 (2026-06-01) — count-badge inner padding `(pad_x, pad_y)` in logical
    /// pixels per tier, matching Tauri's `.zen-capsule__badge { padding }`
    /// (`ZenCapsule.css:40` medium `2px 9px`; `:95` small `1px 6px`; `:115`
    /// large `3px 12px`). Order is `(horizontal, vertical)` to mirror the CSS
    /// `padding: <v> <h>` shorthand split.
    #[inline]
    pub const fn badge_padding_xy(self) -> (f32, f32) {
        match self {
            Self::Small => (6.0, 1.0),
            Self::Medium => (9.0, 2.0),
            Self::Large => (12.0, 3.0),
        }
    }

    /// G5 (2026-06-01) — count-badge box height in logical pixels per tier,
    /// derived from Tauri's `font-size * line-height(1.4) + 2*pad_y`
    /// (`ZenCapsule.css:35-42`): small `10*1.4 + 2`≈16→**14**, medium
    /// `11*1.4 + 4`≈19→**16**, large `13*1.4 + 6`≈24→**20**. Rounded to the
    /// nearest readable even DIP per tier (small 14 / medium 16 / large 20).
    /// Replaces the flat `PILL_BADGE_HEIGHT`=20 used on every tier.
    #[inline]
    pub const fn badge_height_px(self) -> f32 {
        match self {
            Self::Small => 14.0,
            Self::Medium => 16.0,
            Self::Large => 20.0,
        }
    }

    /// G5 (2026-06-01) — icon glyph size in logical pixels when the shape is
    /// `Circle`, matching Tauri's `.zen-capsule--circle .zen-capsule__icon`
    /// override (`ZenCapsule.css:68-70` = 22 for small+medium; `:127-129`
    /// large = 28). The circle branch must NOT reuse [`icon_px`] (14/18/22);
    /// only the non-circle shapes use the per-tier base icon size.
    #[inline]
    pub const fn circle_icon_px(self) -> f32 {
        match self {
            Self::Small | Self::Medium => 22.0,
            Self::Large => 28.0,
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
    fn pad_lr_table_matches_tauri() {
        // G5 — asymmetric padding per tier (ZenCapsule.css:8/:81/:101).
        assert_eq!(CapsuleSize::Small.pad_lr_px(), (12.0, 12.0));
        assert_eq!(CapsuleSize::Medium.pad_lr_px(), (20.0, 16.0));
        assert_eq!(CapsuleSize::Large.pad_lr_px(), (28.0, 20.0));
    }

    #[test]
    fn inner_gap_table_matches_tauri() {
        // G5 — single `gap` token per tier (ZenCapsule.css:9/:82/:102).
        assert!((CapsuleSize::Small.inner_gap_px() - 8.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.inner_gap_px() - 12.0).abs() < 0.01);
        assert!((CapsuleSize::Large.inner_gap_px() - 16.0).abs() < 0.01);
    }

    #[test]
    fn title_font_table_matches_tauri() {
        // G5 — title font-size per tier (ZenCapsule.css:25/:90/:110).
        assert!((CapsuleSize::Small.title_font_px() - 11.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.title_font_px() - 14.0).abs() < 0.01);
        assert!((CapsuleSize::Large.title_font_px() - 16.0).abs() < 0.01);
    }

    #[test]
    fn badge_font_table_matches_tauri() {
        // G5 — badge font-size per tier (ZenCapsule.css:35/:94/:114), weight
        // semibold 600 on every tier (:36).
        assert!((CapsuleSize::Small.badge_font_px() - 10.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.badge_font_px() - 11.0).abs() < 0.01);
        assert!((CapsuleSize::Large.badge_font_px() - 13.0).abs() < 0.01);
        for s in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
            assert_eq!(s.badge_font_weight(), 600);
        }
    }

    #[test]
    fn badge_padding_and_height_tables_match_tauri() {
        // G5 — badge padding (h, v) per tier (ZenCapsule.css:40/:95/:115).
        assert_eq!(CapsuleSize::Small.badge_padding_xy(), (6.0, 1.0));
        assert_eq!(CapsuleSize::Medium.badge_padding_xy(), (9.0, 2.0));
        assert_eq!(CapsuleSize::Large.badge_padding_xy(), (12.0, 3.0));
        // Box height derived from font*1.4 + 2*pad_y, rounded per tier.
        assert!((CapsuleSize::Small.badge_height_px() - 14.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.badge_height_px() - 16.0).abs() < 0.01);
        assert!((CapsuleSize::Large.badge_height_px() - 20.0).abs() < 0.01);
    }

    #[test]
    fn circle_icon_table_matches_tauri() {
        // G5 — circle icon override (ZenCapsule.css:68-70 = 22 small+medium,
        // :127-129 large = 28). Distinct from the base icon_px (14/18/22).
        assert!((CapsuleSize::Small.circle_icon_px() - 22.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.circle_icon_px() - 22.0).abs() < 0.01);
        assert!((CapsuleSize::Large.circle_icon_px() - 28.0).abs() < 0.01);
        // The circle override must NOT equal the base icon size for small/med.
        assert_ne!(
            CapsuleSize::Small.circle_icon_px(),
            CapsuleSize::Small.icon_px()
        );
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
