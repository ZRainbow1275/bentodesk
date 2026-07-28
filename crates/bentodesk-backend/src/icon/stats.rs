//! T-080 — runtime statistics for the tiered icon cache.
//!
//! Direct port of `bentodesk/src-tauri/src/icon/stats.rs`. No deps on
//! Tauri or app state — uses only `std::sync::atomic` so any thread can
//! read/write counters without acquiring the cache lock.
//!
//! Snapshot type derives `serde::Serialize/Deserialize` per master plan
//! §11 ΔB — single-process Phase 1 never serializes at runtime, but
//! v2.x scripting hooks need the shape preserved.

use core::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};

/// Per-cache atomic counters. Construct via `default()`; counters are
/// updated through the `record_*` methods (each is a single relaxed
/// `fetch_add`, cheap enough to call from the hottest hit/miss path).
#[derive(Debug, Default)]
pub struct IconCacheStats {
    hot_hits: AtomicU64,
    warm_hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    warm_writes: AtomicU64,
    warm_write_failures: AtomicU64,
}

/// Snapshot of counter values — safe to serialize and expose over the
/// dispatcher's command bus.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct IconCacheStatsSnapshot {
    pub hot_hits: u64,
    pub warm_hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub warm_writes: u64,
    pub warm_write_failures: u64,
    pub total_lookups: u64,
    /// `(hot + warm) / (hot + warm + miss)`. `0.0` when there have been
    /// no lookups yet.
    pub hit_rate: f64,
}

impl IconCacheStats {
    pub fn record_hot_hit(&self) {
        self.hot_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_warm_hit(&self) {
        self.warm_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_warm_write(&self) {
        self.warm_writes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_warm_write_failure(&self) {
        self.warm_write_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> IconCacheStatsSnapshot {
        let hot = self.hot_hits.load(Ordering::Relaxed);
        let warm = self.warm_hits.load(Ordering::Relaxed);
        let miss = self.misses.load(Ordering::Relaxed);
        let total = hot + warm + miss;
        let hit_rate = if total == 0 {
            0.0
        } else {
            (hot + warm) as f64 / total as f64
        };
        IconCacheStatsSnapshot {
            hot_hits: hot,
            warm_hits: warm,
            misses: miss,
            evictions: self.evictions.load(Ordering::Relaxed),
            warm_writes: self.warm_writes.load(Ordering::Relaxed),
            warm_write_failures: self.warm_write_failures.load(Ordering::Relaxed),
            total_lookups: total,
            hit_rate,
        }
    }

    pub fn reset(&self) {
        self.hot_hits.store(0, Ordering::Relaxed);
        self.warm_hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.warm_writes.store(0, Ordering::Relaxed);
        self.warm_write_failures.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_returns_zero_hit_rate() {
        let s = IconCacheStats::default();
        let snap = s.snapshot();
        assert_eq!(snap.total_lookups, 0);
        assert_eq!(snap.hit_rate, 0.0);
    }

    #[test]
    fn hit_rate_computed_correctly() {
        let s = IconCacheStats::default();
        s.record_hot_hit();
        s.record_hot_hit();
        s.record_warm_hit();
        s.record_miss();
        let snap = s.snapshot();
        assert_eq!(snap.total_lookups, 4);
        assert!((snap.hit_rate - 0.75).abs() < 1e-9);
    }

    #[test]
    fn reset_clears_all_counters() {
        let s = IconCacheStats::default();
        s.record_hot_hit();
        s.record_miss();
        s.record_eviction();
        s.reset();
        let snap = s.snapshot();
        assert_eq!(snap.hot_hits, 0);
        assert_eq!(snap.misses, 0);
        assert_eq!(snap.evictions, 0);
    }

    #[test]
    fn snapshot_is_serde_round_trip() {
        let s = IconCacheStats::default();
        s.record_hot_hit();
        let snap = s.snapshot();
        let json = serde_json::to_string(&snap).expect("serialize");
        let back: IconCacheStatsSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.hot_hits, snap.hot_hits);
        assert_eq!(back.total_lookups, snap.total_lookups);
    }
}
