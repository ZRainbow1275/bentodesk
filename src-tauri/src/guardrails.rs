//! Runtime safety limits that bound memory and UI growth.
//!
//! These limits are intentionally generous for normal use, but they prevent
//! pathological layouts from overwhelming the process with thousands of zones,
//! unbounded item counts, or oversized icon caches.

use serde::Serialize;

use crate::config::settings::{AppSettings, SafetyProfile};
use crate::layout::persistence::{BentoZone, LayoutData};

/// Absolute compile-time cap on the total number of zones.
pub const ABSOLUTE_MAX_ZONES: usize = 384;

/// Absolute compile-time cap on how many items a single zone may contain.
pub const ABSOLUTE_MAX_ITEMS_PER_ZONE: usize = 3_072;

/// Absolute compile-time cap on the total item count across the entire layout.
pub const ABSOLUTE_MAX_TOTAL_ITEMS: usize = 24_576;

/// Lowest allowed in-memory icon cache capacity.
pub const MIN_ICON_CACHE_SIZE: u32 = 64;

/// Highest allowed in-memory icon cache capacity.
pub const MAX_ICON_CACHE_SIZE: u32 = 4_096;

/// Absolute compile-time cap on how many desktop entries smart grouping scans
/// may return to the frontend in one pass.
pub const ABSOLUTE_MAX_SCAN_ENTRIES: usize = 12_000;

/// Effective runtime safety envelope derived from the current settings profile.
#[derive(Debug, Clone, Copy)]
pub struct GuardrailConfig {
    pub profile: SafetyProfile,
    pub max_zones: usize,
    pub max_items_per_zone: usize,
    pub max_total_items: usize,
    pub max_icon_cache_size: u32,
    pub max_scan_entries: usize,
}

/// Diagnostics payload for the runtime safety envelope.
#[derive(Debug, Clone, Serialize)]
pub struct GuardrailInfo {
    pub profile: String,
    pub current_zone_count: usize,
    pub current_total_items: usize,
    pub max_zones: usize,
    pub max_items_per_zone: usize,
    pub max_total_items: usize,
    pub min_icon_cache_size: u32,
    pub max_icon_cache_size: u32,
    pub max_scan_entries: usize,
}

/// Count every item across all zones.
pub fn total_items(layout: &LayoutData) -> usize {
    layout.zones.iter().map(|zone| zone.items.len()).sum()
}

/// Resolve the current runtime safety envelope from the active settings.
pub fn config_for_settings(settings: &AppSettings) -> GuardrailConfig {
    config_for_profile(settings.safety_profile)
}

/// Resolve the runtime safety envelope for a specific profile.
pub fn config_for_profile(profile: SafetyProfile) -> GuardrailConfig {
    match profile {
        SafetyProfile::Conservative => GuardrailConfig {
            profile,
            max_zones: 128,
            max_items_per_zone: 1_024,
            max_total_items: 8_192,
            max_icon_cache_size: 512,
            max_scan_entries: 2_048,
        },
        SafetyProfile::Balanced => GuardrailConfig {
            profile,
            max_zones: 256,
            max_items_per_zone: 2_048,
            max_total_items: 16_384,
            max_icon_cache_size: 1_024,
            max_scan_entries: 4_096,
        },
        SafetyProfile::Expanded => GuardrailConfig {
            profile,
            max_zones: ABSOLUTE_MAX_ZONES,
            max_items_per_zone: ABSOLUTE_MAX_ITEMS_PER_ZONE,
            max_total_items: ABSOLUTE_MAX_TOTAL_ITEMS,
            max_icon_cache_size: MAX_ICON_CACHE_SIZE,
            max_scan_entries: ABSOLUTE_MAX_SCAN_ENTRIES,
        },
    }
}

/// Clamp large smart-group scan results before they are sent to the frontend.
pub fn clamp_scan_result_count<T>(
    entries: &mut Vec<T>,
    settings: &AppSettings,
    operation: &str,
) -> usize {
    let config = config_for_settings(settings);
    let limit = config.max_scan_entries;
    if entries.len() <= limit {
        return 0;
    }

    let dropped = entries.len() - limit;
    entries.truncate(limit);
    tracing::warn!(
        operation,
        profile = ?config.profile,
        limit,
        dropped,
        "Truncated smart-group scan results to stay inside the frontend safety envelope"
    );
    dropped
}

/// Return the maximum additional items this zone can accept right now.
pub fn additional_item_capacity(
    layout: &LayoutData,
    settings: &AppSettings,
    zone_id: &str,
) -> Result<usize, String> {
    let config = config_for_settings(settings);
    let zone = zone_by_id(layout, zone_id)?;
    let zone_remaining = config.max_items_per_zone.saturating_sub(zone.items.len());
    let total_remaining = config.max_total_items.saturating_sub(total_items(layout));
    Ok(zone_remaining.min(total_remaining))
}

/// Ensure creating one more zone remains within the safety envelope.
pub fn ensure_can_create_zone(layout: &LayoutData, settings: &AppSettings) -> Result<(), String> {
    let config = config_for_settings(settings);
    if layout.zones.len() >= config.max_zones {
        return Err(format!(
            "Cannot create more zones: BentoDesk safety profile '{:?}' is capped at {} zones to protect memory and layout stability.",
            config.profile, config.max_zones
        ));
    }

    Ok(())
}

/// Ensure adding `additional` items to a zone remains within the safety envelope.
pub fn ensure_can_add_items(
    layout: &LayoutData,
    settings: &AppSettings,
    zone_id: &str,
    additional: usize,
) -> Result<(), String> {
    let config = config_for_settings(settings);
    let zone = zone_by_id(layout, zone_id)?;
    let zone_len = zone.items.len();
    let total_len = total_items(layout);

    if zone_len.saturating_add(additional) > config.max_items_per_zone {
        return Err(format!(
            "Cannot add more items to zone '{}': safety profile '{:?}' caps this zone at {} items.",
            zone.name, config.profile, config.max_items_per_zone
        ));
    }

    if total_len.saturating_add(additional) > config.max_total_items {
        return Err(format!(
            "Cannot add more items: safety profile '{:?}' caps BentoDesk at {} total items to protect memory and responsiveness.",
            config.profile, config.max_total_items
        ));
    }

    Ok(())
}

/// Ensure moving one more item into the target zone remains safe.
pub fn ensure_can_move_item_into_zone(
    layout: &LayoutData,
    settings: &AppSettings,
    zone_id: &str,
) -> Result<(), String> {
    let config = config_for_settings(settings);
    let zone = zone_by_id(layout, zone_id)?;
    if zone.items.len() >= config.max_items_per_zone {
        return Err(format!(
            "Cannot move more items into zone '{}': safety profile '{:?}' caps this zone at {} items.",
            zone.name, config.profile, config.max_items_per_zone
        ));
    }

    Ok(())
}

/// Clamp icon cache size into the active profile's safe range.
pub fn clamp_icon_cache_size(size: u32, settings: &AppSettings) -> u32 {
    let config = config_for_settings(settings);
    size.clamp(MIN_ICON_CACHE_SIZE, config.max_icon_cache_size)
}

/// Build a diagnostics snapshot for the current layout state.
pub fn guardrail_info(layout: &LayoutData, settings: &AppSettings) -> GuardrailInfo {
    let config = config_for_settings(settings);
    GuardrailInfo {
        profile: format!("{:?}", config.profile),
        current_zone_count: layout.zones.len(),
        current_total_items: total_items(layout),
        max_zones: config.max_zones,
        max_items_per_zone: config.max_items_per_zone,
        max_total_items: config.max_total_items,
        min_icon_cache_size: MIN_ICON_CACHE_SIZE,
        max_icon_cache_size: config.max_icon_cache_size,
        max_scan_entries: config.max_scan_entries,
    }
}

fn zone_by_id<'a>(layout: &'a LayoutData, zone_id: &str) -> Result<&'a BentoZone, String> {
    layout
        .zones
        .iter()
        .find(|zone| zone.id == zone_id)
        .ok_or_else(|| format!("Zone not found: {zone_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::persistence::{
        BentoItem, GridPosition, ItemType, RelativePosition, RelativeSize,
    };

    fn make_item(id: usize) -> BentoItem {
        BentoItem {
            id: format!("item-{id}"),
            zone_id: "zone-1".to_string(),
            item_type: ItemType::File,
            name: format!("Item {id}"),
            path: format!("C:\\Desktop\\Item{id}.txt"),
            icon_hash: format!("hash-{id}"),
            grid_position: GridPosition {
                col: 0,
                row: id as u32,
                col_span: 1,
            },
            is_wide: false,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            original_path: None,
            hidden_path: None,
            icon_x: None,
            icon_y: None,
            file_missing: false,
        }
    }

    fn make_layout(zone_count: usize, items_per_zone: usize) -> LayoutData {
        LayoutData {
            version: "1.0.0".to_string(),
            coherence_id: None,
            zones: (0..zone_count)
                .map(|zone_idx| crate::layout::persistence::BentoZone {
                    id: format!("zone-{zone_idx}"),
                    name: format!("Zone {zone_idx}"),
                    icon: "D".to_string(),
                    position: RelativePosition {
                        x_percent: 0.0,
                        y_percent: 0.0,
                    },
                    expanded_size: RelativeSize {
                        w_percent: 20.0,
                        h_percent: 20.0,
                    },
                    items: (0..items_per_zone).map(make_item).collect(),
                    accent_color: None,
                    sort_order: zone_idx as i32,
                    auto_group: None,
                    grid_columns: 4,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    updated_at: "2026-01-01T00:00:00Z".to_string(),
                    capsule_size: "medium".to_string(),
                    capsule_shape: "pill".to_string(),
                    locked: false,
                    stack_id: None,
                    stack_order: 0,
                    alias: None,
                    display_mode: None,
                    live_folder_path: None,
                })
                .collect(),
            last_modified: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_settings(profile: SafetyProfile) -> AppSettings {
        AppSettings {
            safety_profile: profile,
            ..AppSettings::default()
        }
    }

    #[test]
    fn create_zone_limit_rejects_overflow() {
        let settings = make_settings(SafetyProfile::Conservative);
        let layout = make_layout(config_for_settings(&settings).max_zones, 0);
        let err = ensure_can_create_zone(&layout, &settings).unwrap_err();
        assert!(err.contains("Conservative"));
    }

    #[test]
    fn add_item_capacity_is_bounded_by_zone_and_total_limits() {
        let settings = make_settings(SafetyProfile::Balanced);
        let zone_limit = config_for_settings(&settings).max_items_per_zone;
        let layout = make_layout(1, zone_limit - 2);
        let remaining = additional_item_capacity(&layout, &settings, "zone-0").unwrap();
        assert_eq!(remaining, 2);
        assert!(ensure_can_add_items(&layout, &settings, "zone-0", 2).is_ok());
        assert!(ensure_can_add_items(&layout, &settings, "zone-0", 3).is_err());
    }

    #[test]
    fn icon_cache_size_is_clamped_to_profile_bounds() {
        let conservative = make_settings(SafetyProfile::Conservative);
        let balanced = make_settings(SafetyProfile::Balanced);
        let expanded = make_settings(SafetyProfile::Expanded);

        assert_eq!(
            clamp_icon_cache_size(32, &conservative),
            MIN_ICON_CACHE_SIZE
        );
        assert_eq!(clamp_icon_cache_size(700, &conservative), 512);
        assert_eq!(clamp_icon_cache_size(2_000, &balanced), 1_024);
        assert_eq!(
            clamp_icon_cache_size(MAX_ICON_CACHE_SIZE + 1, &expanded),
            MAX_ICON_CACHE_SIZE
        );
    }

    #[test]
    fn expanded_profile_uses_absolute_caps() {
        let config = config_for_profile(SafetyProfile::Expanded);
        assert_eq!(config.max_zones, ABSOLUTE_MAX_ZONES);
        assert_eq!(config.max_items_per_zone, ABSOLUTE_MAX_ITEMS_PER_ZONE);
        assert_eq!(config.max_total_items, ABSOLUTE_MAX_TOTAL_ITEMS);
        assert_eq!(config.max_icon_cache_size, MAX_ICON_CACHE_SIZE);
        assert_eq!(config.max_scan_entries, ABSOLUTE_MAX_SCAN_ENTRIES);
    }

    #[test]
    fn clamp_scan_result_count_truncates_to_profile_limit() {
        let settings = make_settings(SafetyProfile::Conservative);
        let limit = config_for_settings(&settings).max_scan_entries;
        let mut entries: Vec<usize> = (0..limit + 10).collect();

        let dropped = clamp_scan_result_count(&mut entries, &settings, "scan_desktop");

        assert_eq!(dropped, 10);
        assert_eq!(entries.len(), limit);
    }
}
