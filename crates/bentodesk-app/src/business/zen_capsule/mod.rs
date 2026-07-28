//! Business surface — `ZenCapsule`, the collapsed pill state of a BentoZone.
//!
//! Visual spec: see `zen_capsule.snap.md`. Three size / three shape variants
//! are locked here as the wire-format contract; downstream layout JSON
//! (`zones[i].appearance`) round-trips through the `CapsuleShape` /
//! `CapsuleSize` enums via serde.
//!
//! The renderer and hit-test consume this wire/geometry model directly.
//! `build()` remains the compatibility widget-tree descriptor; native capsule
//! chrome, icon, title and badge painting live in `render::zone_chrome`.

use bentodesk_layout::Direction;
use bentodesk_style::{Edges, Length};
use bentodesk_widget::{ContainerNode, WidgetNode};
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
    ///   is native's own near-sharp treatment retained for back-compat).
    ///
    /// Stack-only `match` on a `Copy` enum (spec §10) — no alloc, no panic.
    #[inline]
    pub fn corner_radius_px(self, height_px: f32) -> f32 {
        match self {
            Self::Pill => 24.0,
            Self::Rounded => 12.0,
            Self::Circle => height_px * 0.5,
            Self::Minimal => 8.0,
            Self::Square => 0.0,
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
    /// Capsule outer height in logical pixels.
    ///
    /// M2② (2026-05-29) — re-centred on Tauri v1.3.0's `getCapsuleBoxPx`
    /// (`bentodesk/src/services/hitTest.ts:94-99`), the authoritative pixel
    /// source for the collapsed capsule box: `small {height:36}` /
    /// `medium {height:48}` / `large {height:56}`. The selected-stack Large
    /// tier intentionally compacts the outer height to 50 DIPs after the
    /// 2026-07-23 hand test: its 200-DIP width remains distinct, while the
    /// former 56-DIP band read disproportionately heavy at 150% DPI.
    pub const fn height_px(self) -> f32 {
        match self {
            Self::Small => 36.0,
            Self::Medium => 48.0,
            Self::Large => 50.0,
        }
    }

    /// Capsule outer width in logical pixels for the non-circle tiers. This is
    /// the original Tauri runtime contract shared by `hitTest.ts`,
    /// `StackWrapper.tsx`, and the recorded DOM geometry: 120 / 160 / 200 DIPs.
    /// Keeping visibly distinct tiers is also essential for the editor's width
    /// control to have an immediately understandable runtime effect.
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

    /// Actual icon box size in logical pixels.
    ///
    /// V21-C4 (2026-06-22) — align to Tauri's real `ZenCapsule.tsx` render
    /// contract, not the unreachable CSS font-size intent. `ZenCapsule` passes
    /// `size={18}` into `ZoneIcon`; `ZoneIcon.css` uses that prop to set
    /// `--zone-icon-size`, and the built-in SVG/custom image fills that 18px
    /// wrapper. The small/large `.zen-capsule__icon { font-size: ... }` rules
    /// apply to the outer span only and do not resize the inner SVG box.
    pub const fn icon_px(self) -> f32 {
        match self {
            Self::Small | Self::Medium | Self::Large => 18.0,
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

    /// Readable title role in logical pixels. Capsule size changes geometry,
    /// not legibility: every tier keeps the same 13-DIP title and lets DWrite
    /// ellipsize when the available line is short. This replaces the former
    /// 11/14/15 table plus 8px shrink floor that made small capsules look like
    /// a different product state rather than a compact size option.
    #[inline]
    pub const fn title_font_px(self) -> f32 {
        match self {
            Self::Small | Self::Medium | Self::Large => 13.0,
        }
    }

    /// Count-badge font size in logical pixels per tier.
    ///
    /// G5 matched the local Tauri CSS source table (`10/11/13`), but the
    /// 2026-06-02 authoritative reference frame shows large Browser/Compiler
    /// count chips with the same visual bbox as medium chips. Keep the large
    /// title and outer capsule large, but use the video-observed medium badge
    /// metrics so the count chip does not overgrow the local reference band.
    #[inline]
    pub const fn badge_font_px(self) -> f32 {
        match self {
            Self::Small => 10.0,
            Self::Medium => 11.0,
            Self::Large => 11.0,
        }
    }

    /// V21-C34 — count-badge weight. Tauri `.zen-capsule__badge` declares
    /// semibold, but the 2026-06-02 reference frame reads heavier than DWrite's
    /// weight-600 digit rasterization at the video-observed 11px badge tier.
    /// Use a heavier DWrite weight step to match the reference ink density
    /// without changing the already-aligned badge box geometry.
    #[inline]
    pub const fn badge_font_weight(self) -> u16 {
        800
    }

    /// Count-badge inner padding `(pad_x, pad_y)` in logical pixels per tier.
    ///
    /// Order is `(horizontal, vertical)` to mirror the CSS `padding: <v> <h>`
    /// shorthand split. Large uses the same chip padding as medium because the
    /// 2026-06-02 reference large capsules show medium-sized count chips.
    #[inline]
    pub const fn badge_padding_xy(self) -> (f32, f32) {
        match self {
            Self::Small => (6.0, 1.0),
            Self::Medium => (9.0, 2.0),
            Self::Large => (9.0, 2.0),
        }
    }

    /// Count-badge box height in logical pixels per tier.
    ///
    /// Small and medium keep the G5 table. C18 reduced the old 20-DIP large
    /// chip, but post-C22 component crops showed 16 DIPs undershot the
    /// 2026-06-02 Browser/Compiler badge vertical span. Large therefore uses a
    /// 17-DIP video-observed midpoint: just taller than medium, shorter than
    /// the source 20-DIP large chip.
    #[inline]
    pub const fn badge_height_px(self) -> f32 {
        match self {
            Self::Small => 14.0,
            Self::Medium => 16.0,
            Self::Large => 17.0,
        }
    }

    /// Icon glyph size in logical pixels when the shape is `Circle`.
    ///
    /// V21-C4 — Tauri still renders the same inner `ZoneIcon size={18}` box in
    /// circle mode. The circle CSS font-size override targets the wrapping
    /// span, not `ZoneIcon`'s `--zone-icon-size`, so the actual SVG/custom image
    /// size remains 18px for every tier.
    #[inline]
    pub const fn circle_icon_px(self) -> f32 {
        self.icon_px()
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
/// (`Pill` / `Medium`) appearance.
pub fn build() -> WidgetNode {
    build_with(CapsuleShape::default(), CapsuleSize::default())
}

/// Build the collapsed-pill subtree with explicit shape + size.
/// Geometry is locked per `zen_capsule.snap.md`.
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
    use bentodesk_layout::LayoutSource;

    #[test]
    fn defaults_match_snap_md() {
        assert_eq!(CapsuleShape::default(), CapsuleShape::Pill);
        assert_eq!(CapsuleSize::default(), CapsuleSize::Medium);
    }

    #[test]
    fn size_height_table_keeps_large_capsule_compact() {
        // Small/medium retain the baseline; Large is the hand-test-polished
        // selected-stack tier (same 200-DIP width, less vertical bulk).
        assert!((CapsuleSize::Small.height_px() - 36.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.height_px() - 48.0).abs() < 0.01);
        assert!((CapsuleSize::Large.height_px() - 50.0).abs() < 0.01);
    }

    #[test]
    fn size_width_table_matches_tauri() {
        assert_eq!(CapsuleSize::Small.width_px(), 120.0);
        assert_eq!(CapsuleSize::Medium.width_px(), 160.0);
        assert_eq!(CapsuleSize::Large.width_px(), 200.0);
    }

    #[test]
    fn size_icon_table_matches_tauri() {
        // V21-C4 — Tauri `ZenCapsule.tsx` passes fixed `size={18}` to ZoneIcon.
        assert!((CapsuleSize::Small.icon_px() - 18.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.icon_px() - 18.0).abs() < 0.01);
        assert!((CapsuleSize::Large.icon_px() - 18.0).abs() < 0.01);
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
    fn title_font_role_stays_readable_across_size_switches() {
        for size in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
            assert!((size.title_font_px() - 13.0).abs() < 0.01);
        }
    }

    #[test]
    fn badge_font_table_matches_video_observed_reference() {
        // C18 — small/medium stay source-aligned; large uses the
        // video-observed medium-sized count chip from the 2026-06-02 Browser /
        // Compiler reference crops.
        assert!((CapsuleSize::Small.badge_font_px() - 10.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.badge_font_px() - 11.0).abs() < 0.01);
        assert!((CapsuleSize::Large.badge_font_px() - 11.0).abs() < 0.01);
        for s in [CapsuleSize::Small, CapsuleSize::Medium, CapsuleSize::Large] {
            assert_eq!(s.badge_font_weight(), 800);
        }
    }

    #[test]
    fn badge_padding_and_height_tables_match_video_observed_reference() {
        // C23 — large count chips keep medium padding/font but use a 17-DIP
        // video-observed vertical span between C18's 16-DIP chip and the source
        // 20-DIP large chip.
        assert_eq!(CapsuleSize::Small.badge_padding_xy(), (6.0, 1.0));
        assert_eq!(CapsuleSize::Medium.badge_padding_xy(), (9.0, 2.0));
        assert_eq!(CapsuleSize::Large.badge_padding_xy(), (9.0, 2.0));
        assert!((CapsuleSize::Small.badge_height_px() - 14.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.badge_height_px() - 16.0).abs() < 0.01);
        assert!((CapsuleSize::Large.badge_height_px() - 17.0).abs() < 0.01);
    }

    #[test]
    fn circle_icon_table_matches_tauri() {
        // V21-C4 — circle keeps the same fixed `ZoneIcon size={18}` inner box;
        // the CSS circle font-size override affects only the outer span.
        assert!((CapsuleSize::Small.circle_icon_px() - 18.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.circle_icon_px() - 18.0).abs() < 0.01);
        assert!((CapsuleSize::Large.circle_icon_px() - 18.0).abs() < 0.01);
        assert_eq!(
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
        assert!((CapsuleShape::Square.corner_radius_px(h) - 0.0).abs() < 0.01);
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
        assert!(matches!(layout.height, Length::Px(h) if (h - 50.0).abs() < 0.01));
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
        let legacy: CapsuleShape = serde_json::from_str("\"square\"").unwrap_or_default();
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
