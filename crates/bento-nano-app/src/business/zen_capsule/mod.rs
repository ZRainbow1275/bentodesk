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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CapsuleShape {
    #[default]
    Pill,
    Rounded,
    Square,
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
    pub const fn height_px(self) -> f32 {
        match self {
            Self::Small => 36.0,
            Self::Medium => 44.0,
            Self::Large => 52.0,
        }
    }

    /// Icon size in logical pixels — snap.md mandated.
    pub const fn icon_px(self) -> f32 {
        match self {
            Self::Small => 20.0,
            Self::Medium => 24.0,
            Self::Large => 28.0,
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
    fn size_height_table_matches_snap_md() {
        assert!((CapsuleSize::Small.height_px() - 36.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.height_px() - 44.0).abs() < 0.01);
        assert!((CapsuleSize::Large.height_px() - 52.0).abs() < 0.01);
    }

    #[test]
    fn size_icon_table_matches_snap_md() {
        assert!((CapsuleSize::Small.icon_px() - 20.0).abs() < 0.01);
        assert!((CapsuleSize::Medium.icon_px() - 24.0).abs() < 0.01);
        assert!((CapsuleSize::Large.icon_px() - 28.0).abs() < 0.01);
    }

    #[test]
    fn build_uses_medium_height() {
        let node = build();
        let layout = node.layout();
        assert_eq!(layout.direction, Direction::Row);
        assert!(matches!(layout.height, Length::Px(h) if (h - 44.0).abs() < 0.01));
    }

    #[test]
    fn build_with_size_overrides_height() {
        let node = build_with(CapsuleShape::Rounded, CapsuleSize::Large);
        let layout = node.layout();
        assert!(matches!(layout.height, Length::Px(h) if (h - 52.0).abs() < 0.01));
    }

    /// Wire-format lock: 1.x `appearance.capsule_shape = "pill"` + serde must
    /// continue to deserialize after the Rust rewrite.
    #[test]
    fn capsule_shape_serde_round_trip() {
        for v in [
            CapsuleShape::Pill,
            CapsuleShape::Rounded,
            CapsuleShape::Square,
        ] {
            let s = serde_json::to_string(&v).unwrap_or_default();
            let back: CapsuleShape = serde_json::from_str(&s).unwrap_or_default();
            assert_eq!(v, back);
        }
        assert_eq!(
            serde_json::to_string(&CapsuleShape::Pill).unwrap_or_default(),
            "\"pill\""
        );
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
