//! Desktop zone surface chrome shared by renderer and parity tests.
//!
//! This keeps the selected-stack main desktop renderer from baking local
//! radius constants into `render.rs`. Colours still belong to the renderer
//! because they are derived from live zone state, drag state, and palette
//! alpha composition; geometry tokens live here so theme switching has a
//! single contract.

use bento_nano_style::BorderRadius;
use bento_nano_theme::{RadiusTokens, radius};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneSurfaceChrome {
    pub zone_radius: BorderRadius,
    pub stack_halo_radius: BorderRadius,
    pub drop_target_radius: BorderRadius,
    pub accent_radius: BorderRadius,
    pub icon_chip_radius: BorderRadius,
    pub display_chip_radius: BorderRadius,
    pub stack_badge_radius: BorderRadius,
    pub stack_peek_radius: BorderRadius,
    pub live_badge_radius: BorderRadius,
    pub drop_preview_core_radius: BorderRadius,
    pub bloom_connector_radius: BorderRadius,
    pub bloom_petal_radius: BorderRadius,
    pub bloom_border_radius: BorderRadius,
    pub bloom_core_radius: BorderRadius,
    pub bloom_index_radius: BorderRadius,
    pub item_status_radius: BorderRadius,
}

impl ZoneSurfaceChrome {
    pub fn from_radius(radius: RadiusTokens) -> Self {
        Self {
            zone_radius: radius.lg,
            stack_halo_radius: radius.xl,
            drop_target_radius: radius.xl,
            accent_radius: radius.sm,
            icon_chip_radius: radius.md,
            display_chip_radius: radius.lg,
            stack_badge_radius: radius.lg,
            stack_peek_radius: radius.lg,
            live_badge_radius: radius.md,
            drop_preview_core_radius: radius.sm,
            bloom_connector_radius: radius.sm,
            bloom_petal_radius: radius.xl,
            bloom_border_radius: radius.xl,
            bloom_core_radius: radius.lg,
            bloom_index_radius: radius.lg,
            item_status_radius: radius.xl,
        }
    }

    pub fn default_theme() -> Self {
        Self::from_radius(radius::DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_preserves_primary_zone_and_chip_radii() {
        let chrome = ZoneSurfaceChrome::default_theme();

        assert_eq!(chrome.zone_radius, BorderRadius::all(8.0));
        assert_eq!(chrome.icon_chip_radius, BorderRadius::all(6.0));
        assert_eq!(chrome.stack_halo_radius, BorderRadius::all(12.0));
    }

    #[test]
    fn chrome_accepts_explicit_active_radius_tokens() {
        let radius = RadiusTokens {
            sm: BorderRadius::all(3.0),
            md: BorderRadius::all(7.0),
            lg: BorderRadius::all(11.0),
            xl: BorderRadius::all(17.0),
            full: BorderRadius::all(999.0),
        };

        let chrome = ZoneSurfaceChrome::from_radius(radius);

        assert_eq!(chrome.zone_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.stack_halo_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.drop_target_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.accent_radius, BorderRadius::all(3.0));
        assert_eq!(chrome.icon_chip_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.display_chip_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.stack_badge_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.stack_peek_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.live_badge_radius, BorderRadius::all(7.0));
        assert_eq!(chrome.drop_preview_core_radius, BorderRadius::all(3.0));
        assert_eq!(chrome.bloom_connector_radius, BorderRadius::all(3.0));
        assert_eq!(chrome.bloom_petal_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.bloom_border_radius, BorderRadius::all(17.0));
        assert_eq!(chrome.bloom_core_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.bloom_index_radius, BorderRadius::all(11.0));
        assert_eq!(chrome.item_status_radius, BorderRadius::all(17.0));
    }
}
