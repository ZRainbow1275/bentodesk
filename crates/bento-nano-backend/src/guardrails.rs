//! T-096 — runtime safety limits that bound memory and UI growth.
//!
//! BentoDesk supports user layouts with hundreds of zones and thousands of
//! items. Without guardrails a pathological layout could:
//!
//! - Allocate so many zones that the renderer's per-zone hibernation
//!   bookkeeping (T-099) exceeds the 100 MB PB ceiling (master plan §1).
//! - Stuff a single zone with so many items that the layout pass spends
//!   more than one frame measuring it.
//! - Push the icon cache past the §5 8-MB LRU envelope.
//! - Stream a desktop-scan result so large that JSON roundtrip (when v2.x
//!   scripting hooks land — ΔB ruling) blows the 128 MB JSON state limit.
//!
//! This module owns the absolute compile-time caps and the per-profile
//! envelopes the user picks via `settings.safety_profile`. The 1.x source
//! reached for `crate::config::settings::AppSettings` directly; the nano
//! port accepts the lightweight [`SafetyProfile`] enum so this crate does
//! not pull in the full settings model (settings live in a higher-layer
//! crate per master plan §11).

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ─── Compile-time absolute limits ────────────────────────────────────

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

/// Absolute compile-time cap on how many desktop entries smart grouping
/// scans may return to the dispatcher in one pass.
pub const ABSOLUTE_MAX_SCAN_ENTRIES: usize = 12_000;

// ─── Profile + envelope ──────────────────────────────────────────────

/// Per-profile envelope of runtime caps. The 1.x source named this
/// `SafetyProfile`; the nano port uses the same variants so on-disk
/// settings round-trip unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyProfile {
    /// Tightest envelope — defaults for users on memory-constrained
    /// hardware. 128 zones / 1024 items per zone / 8K total.
    Conservative,
    /// Default envelope. 256 zones / 2048 items per zone / 16K total.
    #[default]
    Balanced,
    /// Loosest envelope — power users with large desktops. Saturates the
    /// `ABSOLUTE_*` caps.
    Expanded,
}

/// Effective runtime safety envelope derived from a [`SafetyProfile`].
#[derive(Debug, Clone, Copy)]
pub struct GuardrailConfig {
    pub profile: SafetyProfile,
    pub max_zones: usize,
    pub max_items_per_zone: usize,
    pub max_total_items: usize,
    pub max_icon_cache_size: u32,
    pub max_scan_entries: usize,
}

/// Diagnostics payload — carries the current envelope alongside the live
/// counts so the settings UI can render a "% of cap" badge.
///
/// `profile` is `SmolStr` (≤22-byte inline) because the variant names are
/// short and we want a stable JSON shape (`"Balanced"` etc.) for v2.x
/// scripting hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailInfo {
    pub profile: SmolStr,
    pub current_zone_count: usize,
    pub current_total_items: usize,
    pub max_zones: usize,
    pub max_items_per_zone: usize,
    pub max_total_items: usize,
    pub min_icon_cache_size: u32,
    pub max_icon_cache_size: u32,
    pub max_scan_entries: usize,
}

// ─── Error type (spec §8.1 — hand-rolled, no thiserror) ──────────────

/// Errors surfaced by the guardrail enforcement helpers.
#[derive(Debug)]
pub enum GuardrailError {
    /// Zone-count cap reached for the active profile.
    ZoneCapReached { profile: SafetyProfile, cap: usize },
    /// A zone's item-count cap would be exceeded by the requested operation.
    ZoneItemCapExceeded {
        profile: SafetyProfile,
        zone_id: SmolStr,
        cap: usize,
    },
    /// The total item-count cap would be exceeded by the requested operation.
    TotalItemCapExceeded { profile: SafetyProfile, cap: usize },
    /// Caller supplied a zone id that does not exist in the layout snapshot.
    ZoneNotFound { zone_id: SmolStr },
}

impl core::fmt::Display for GuardrailError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZoneCapReached { profile, cap } => write!(
                f,
                "cannot create more zones: safety profile {profile:?} caps the layout at {cap} zones"
            ),
            Self::ZoneItemCapExceeded {
                profile,
                zone_id,
                cap,
            } => write!(
                f,
                "cannot add more items to zone {zone_id}: safety profile {profile:?} caps each zone at {cap} items"
            ),
            Self::TotalItemCapExceeded { profile, cap } => write!(
                f,
                "cannot add more items: safety profile {profile:?} caps BentoDesk at {cap} total items"
            ),
            Self::ZoneNotFound { zone_id } => write!(f, "zone not found: {zone_id}"),
        }
    }
}

impl core::error::Error for GuardrailError {}

// ─── Caller-supplied layout summary ──────────────────────────────────

/// Minimal layout summary the guardrail helpers need. The 1.x source took
/// `&LayoutData` and reached for `zone.items.len()`; the nano port accepts
/// this lightweight value so the guardrail crate doesn't need the full
/// persistence model in scope.
///
/// Construct via [`LayoutCounts::from_zone_sizes`] — caller provides an
/// iterator of `(zone_id, item_count)` and we pre-sum the totals.
#[derive(Debug, Clone, Default)]
pub struct LayoutCounts {
    zones: Vec<(SmolStr, usize)>,
    total_items: usize,
}

impl LayoutCounts {
    /// Build a snapshot from `(zone_id, item_count)` pairs.
    pub fn from_zone_sizes<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = (S, usize)>,
        S: Into<SmolStr>,
    {
        let mut zones: Vec<(SmolStr, usize)> = Vec::new();
        let mut total: usize = 0;
        for (id, count) in iter {
            total = total.saturating_add(count);
            zones.push((id.into(), count));
        }
        Self {
            zones,
            total_items: total,
        }
    }

    /// Number of zones currently in the layout.
    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }

    /// Sum of `items.len()` across every zone.
    pub fn total_items(&self) -> usize {
        self.total_items
    }

    fn zone_size(&self, zone_id: &str) -> Option<usize> {
        self.zones
            .iter()
            .find(|(id, _)| id == zone_id)
            .map(|(_, n)| *n)
    }
}

// ─── Profile envelope resolver ───────────────────────────────────────

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

// ─── Enforcement helpers ─────────────────────────────────────────────

/// Clamp a smart-group scan result before it is sent to the dispatcher.
/// Returns the count of dropped entries (0 when nothing was truncated).
pub fn clamp_scan_result_count<T>(
    entries: &mut Vec<T>,
    profile: SafetyProfile,
    operation: &str,
) -> usize {
    let config = config_for_profile(profile);
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
        "truncated smart-group scan to stay inside safety envelope"
    );
    dropped
}

/// Compute the additional-item capacity remaining for `zone_id` under the
/// active profile. Returns `0` when the zone is at cap or the layout total
/// is at cap.
pub fn additional_item_capacity(
    counts: &LayoutCounts,
    profile: SafetyProfile,
    zone_id: &str,
) -> Result<usize, GuardrailError> {
    let config = config_for_profile(profile);
    let zone_size = counts
        .zone_size(zone_id)
        .ok_or_else(|| GuardrailError::ZoneNotFound {
            zone_id: SmolStr::from(zone_id),
        })?;
    let zone_remaining = config.max_items_per_zone.saturating_sub(zone_size);
    let total_remaining = config.max_total_items.saturating_sub(counts.total_items());
    Ok(zone_remaining.min(total_remaining))
}

/// Reject zone creation when the active profile is already at cap.
pub fn ensure_can_create_zone(
    counts: &LayoutCounts,
    profile: SafetyProfile,
) -> Result<(), GuardrailError> {
    let config = config_for_profile(profile);
    if counts.zone_count() >= config.max_zones {
        return Err(GuardrailError::ZoneCapReached {
            profile,
            cap: config.max_zones,
        });
    }
    Ok(())
}

/// Reject an `additional`-item batch insert when either the per-zone or
/// the layout-total cap would be exceeded.
pub fn ensure_can_add_items(
    counts: &LayoutCounts,
    profile: SafetyProfile,
    zone_id: &str,
    additional: usize,
) -> Result<(), GuardrailError> {
    let config = config_for_profile(profile);
    let zone_size = counts
        .zone_size(zone_id)
        .ok_or_else(|| GuardrailError::ZoneNotFound {
            zone_id: SmolStr::from(zone_id),
        })?;
    if zone_size.saturating_add(additional) > config.max_items_per_zone {
        return Err(GuardrailError::ZoneItemCapExceeded {
            profile,
            zone_id: SmolStr::from(zone_id),
            cap: config.max_items_per_zone,
        });
    }
    if counts.total_items().saturating_add(additional) > config.max_total_items {
        return Err(GuardrailError::TotalItemCapExceeded {
            profile,
            cap: config.max_total_items,
        });
    }
    Ok(())
}

/// Reject a single-item move into the target zone when its per-zone cap is
/// already saturated. Convenience wrapper over [`ensure_can_add_items`]
/// for the drag-drop hot path which always inserts exactly one item.
pub fn ensure_can_move_item_into_zone(
    counts: &LayoutCounts,
    profile: SafetyProfile,
    zone_id: &str,
) -> Result<(), GuardrailError> {
    let config = config_for_profile(profile);
    let zone_size = counts
        .zone_size(zone_id)
        .ok_or_else(|| GuardrailError::ZoneNotFound {
            zone_id: SmolStr::from(zone_id),
        })?;
    if zone_size >= config.max_items_per_zone {
        return Err(GuardrailError::ZoneItemCapExceeded {
            profile,
            zone_id: SmolStr::from(zone_id),
            cap: config.max_items_per_zone,
        });
    }
    Ok(())
}

/// Clamp an icon-cache size into the active profile's safe range.
/// Always returns a value in `[MIN_ICON_CACHE_SIZE, max_for_profile]`.
pub fn clamp_icon_cache_size(size: u32, profile: SafetyProfile) -> u32 {
    let config = config_for_profile(profile);
    size.clamp(MIN_ICON_CACHE_SIZE, config.max_icon_cache_size)
}

/// Build a diagnostics snapshot for the current layout state.
pub fn guardrail_info(counts: &LayoutCounts, profile: SafetyProfile) -> GuardrailInfo {
    let config = config_for_profile(profile);
    GuardrailInfo {
        profile: profile_name(profile),
        current_zone_count: counts.zone_count(),
        current_total_items: counts.total_items(),
        max_zones: config.max_zones,
        max_items_per_zone: config.max_items_per_zone,
        max_total_items: config.max_total_items,
        min_icon_cache_size: MIN_ICON_CACHE_SIZE,
        max_icon_cache_size: config.max_icon_cache_size,
        max_scan_entries: config.max_scan_entries,
    }
}

fn profile_name(profile: SafetyProfile) -> SmolStr {
    match profile {
        SafetyProfile::Conservative => SmolStr::new_static("Conservative"),
        SafetyProfile::Balanced => SmolStr::new_static("Balanced"),
        SafetyProfile::Expanded => SmolStr::new_static("Expanded"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_with(zone_count: usize, items_per_zone: usize) -> LayoutCounts {
        LayoutCounts::from_zone_sizes(
            (0..zone_count).map(|i| (format!("zone-{i}"), items_per_zone)),
        )
    }

    #[test]
    fn create_zone_limit_rejects_overflow_in_conservative_profile() {
        let counts = layout_with(config_for_profile(SafetyProfile::Conservative).max_zones, 0);
        let err = ensure_can_create_zone(&counts, SafetyProfile::Conservative).unwrap_err();
        assert!(matches!(
            err,
            GuardrailError::ZoneCapReached {
                profile: SafetyProfile::Conservative,
                ..
            }
        ));
    }

    #[test]
    fn add_item_capacity_is_bounded_by_zone_and_total() {
        let limit = config_for_profile(SafetyProfile::Balanced).max_items_per_zone;
        let counts = layout_with(1, limit - 2);
        let remaining =
            additional_item_capacity(&counts, SafetyProfile::Balanced, "zone-0").unwrap();
        assert_eq!(remaining, 2);
        assert!(ensure_can_add_items(&counts, SafetyProfile::Balanced, "zone-0", 2).is_ok());
        assert!(ensure_can_add_items(&counts, SafetyProfile::Balanced, "zone-0", 3).is_err());
    }

    #[test]
    fn icon_cache_size_clamps_to_profile_bounds() {
        assert_eq!(
            clamp_icon_cache_size(32, SafetyProfile::Conservative),
            MIN_ICON_CACHE_SIZE
        );
        assert_eq!(clamp_icon_cache_size(700, SafetyProfile::Conservative), 512);
        assert_eq!(clamp_icon_cache_size(2_000, SafetyProfile::Balanced), 1_024);
        assert_eq!(
            clamp_icon_cache_size(MAX_ICON_CACHE_SIZE + 1, SafetyProfile::Expanded),
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
        let limit = config_for_profile(SafetyProfile::Conservative).max_scan_entries;
        let mut entries: Vec<usize> = (0..limit + 10).collect();
        let dropped =
            clamp_scan_result_count(&mut entries, SafetyProfile::Conservative, "scan_desktop");
        assert_eq!(dropped, 10);
        assert_eq!(entries.len(), limit);
    }

    #[test]
    fn unknown_zone_is_zone_not_found() {
        let counts = layout_with(1, 10);
        let err =
            additional_item_capacity(&counts, SafetyProfile::Balanced, "missing").unwrap_err();
        assert!(matches!(err, GuardrailError::ZoneNotFound { .. }));
    }

    #[test]
    fn ensure_can_move_into_zone_at_cap_rejects() {
        let cap = config_for_profile(SafetyProfile::Balanced).max_items_per_zone;
        let counts = LayoutCounts::from_zone_sizes(std::iter::once(("zone-full", cap)));
        let err = ensure_can_move_item_into_zone(&counts, SafetyProfile::Balanced, "zone-full")
            .unwrap_err();
        assert!(matches!(err, GuardrailError::ZoneItemCapExceeded { .. }));
    }

    #[test]
    fn guardrail_info_round_trips_via_serde() {
        let counts = layout_with(2, 5);
        let info = guardrail_info(&counts, SafetyProfile::Balanced);
        let json = serde_json::to_string(&info).unwrap();
        let parsed: GuardrailInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.current_zone_count, 2);
        assert_eq!(parsed.current_total_items, 10);
        assert_eq!(parsed.profile.as_str(), "Balanced");
    }

    #[test]
    fn safety_profile_round_trips_via_serde() {
        let json = serde_json::to_string(&SafetyProfile::Expanded).unwrap();
        let parsed: SafetyProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SafetyProfile::Expanded);
    }
}
