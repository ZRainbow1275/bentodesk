use lru::LruCache;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::cache_tier::WarmTier;
use super::stats::{IconCacheStats, IconCacheStatsSnapshot};

/// 32 MB total byte budget for the in-memory (hot) tier.
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;

/// Two-tier memory/disk cache for extracted icon PNGs.
///
/// Hot tier: `LruCache<String, Arc<Vec<u8>>>` — zero-copy fan-out via
///   [`Arc::clone`]; protocol handler, preloader, and `get_icon_url`
///   can all share the same buffer without cloning bytes.
///
/// Warm tier: [`WarmTier`] — on-disk PNG under
///   `<warm_dir>/<hh>/<hash>.png`. LRU evictions from the hot tier do
///   NOT delete the warm copy, so a subsequent `get()` re-hydrates
///   without another `ExtractIconExW` round-trip.
///
/// Two eviction limits apply to the hot tier:
/// 1. Entry count — classic LRU capacity.
/// 2. Total bytes — the sum of all stored `Arc<Vec<u8>>` payloads must
///    not exceed [`MAX_TOTAL_BYTES`]. When an insert would push the
///    total over the limit, the least-recently-used entries are
///    evicted until the budget is satisfied.
pub struct IconCache {
    inner: Mutex<LruCache<String, Arc<Vec<u8>>>>,
    /// Running total of bytes stored across all values in the hot tier.
    total_bytes: Mutex<usize>,
    warm: Option<WarmTier>,
    stats: IconCacheStats,
}

impl IconCache {
    /// Create a new icon cache with only the in-memory tier. Used by
    /// tests and older call-sites that don't have an app-data path
    /// resolved yet.
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity must be > 0");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
            total_bytes: Mutex::new(0),
            warm: None,
            stats: IconCacheStats::default(),
        }
    }

    /// Create a new icon cache with both hot (in-memory) and warm
    /// (on-disk) tiers enabled. `warm_dir` is created lazily on first
    /// write.
    pub fn with_warm_dir(capacity: usize, warm_dir: PathBuf) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity must be > 0");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
            total_bytes: Mutex::new(0),
            warm: Some(WarmTier::new(warm_dir)),
            stats: IconCacheStats::default(),
        }
    }

    /// Retrieve a cached icon by its hash key.
    ///
    /// Order: hot tier → warm tier (promoted on hit) → `None`.
    /// Statistics are updated on every call.
    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        {
            let mut cache = self.inner.lock().expect("icon cache lock poisoned");
            if let Some(arc) = cache.get(key).cloned() {
                self.stats.record_hot_hit();
                return Some(arc);
            }
        }
        if let Some(warm) = &self.warm {
            if let Some(bytes) = warm.read(key) {
                self.stats.record_warm_hit();
                let arc = Arc::new(bytes);
                self.put_hot_only(key.to_string(), Arc::clone(&arc));
                return Some(arc);
            }
        }
        self.stats.record_miss();
        None
    }

    /// Retrieve a cached icon's raw bytes (clone of the `Arc` payload).
    /// Exists for callers that need owned bytes (e.g. IPC responses
    /// serialised as base64). Prefer [`IconCache::get`] for zero-copy.
    pub fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.get(key).map(|arc| arc.as_ref().clone())
    }

    /// Insert an icon into the hot tier only (no warm write). Used for
    /// warm-tier → hot-tier promotions where the bytes are already on
    /// disk.
    fn put_hot_only(&self, key: String, arc: Arc<Vec<u8>>) {
        let incoming_len = arc.len();
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");

        if let Some(old) = cache.peek(&key) {
            *total = total.saturating_sub(old.len());
        }
        while *total + incoming_len > MAX_TOTAL_BYTES {
            if let Some((_k, evicted)) = cache.pop_lru() {
                *total = total.saturating_sub(evicted.len());
                self.stats.record_eviction();
            } else {
                break;
            }
        }
        cache.put(key, arc);
        *total += incoming_len;
    }

    /// Insert an icon into the cache.
    ///
    /// Writes to both tiers: hot in-memory immediately, warm-tier write
    /// happens synchronously (file I/O is small + cheap — no need for
    /// a spawn overhead for a 1-3 KB PNG).
    ///
    /// If the new entry would push total cached bytes above
    /// [`MAX_TOTAL_BYTES`], the least-recently-used entries are evicted
    /// from the hot tier until the budget is met. **Warm-tier copies
    /// of evicted entries are preserved** so a subsequent lookup can
    /// re-hydrate without re-extracting.
    pub fn put(&self, key: String, data: Vec<u8>) {
        let arc = Arc::new(data);
        if let Some(warm) = &self.warm {
            match warm.write(&key, arc.as_ref()) {
                Ok(_) => self.stats.record_warm_write(),
                Err(e) => {
                    self.stats.record_warm_write_failure();
                    tracing::debug!("warm-tier write failed for {}: {}", key, e);
                }
            }
        }
        self.put_hot_only(key, arc);
    }

    /// Check whether a key is present in the hot tier (warm tier not
    /// consulted — this is a fast-path predicate for the extractor).
    pub fn contains(&self, key: &str) -> bool {
        let cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.contains(key)
    }

    /// True iff the key is present in either hot or warm tier.
    /// Used by the extractor to skip re-extraction when the warm tier
    /// already has bytes we can serve.
    pub fn contains_any_tier(&self, key: &str) -> bool {
        if self.contains(key) {
            return true;
        }
        self.warm.as_ref().map(|w| w.contains(key)).unwrap_or(false)
    }

    /// Remove a single entry from both tiers by key.
    pub fn remove(&self, key: &str) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        if let Some(removed) = cache.pop(key) {
            let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
            *total = total.saturating_sub(removed.len());
        }
        if let Some(warm) = &self.warm {
            warm.remove(key);
        }
    }

    /// Clear all cached icons (hot tier + warm tier).
    pub fn clear(&self) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.clear();
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total = 0;
        if let Some(warm) = &self.warm {
            if let Err(e) = warm.clear() {
                tracing::warn!("failed to clear warm icon tier: {e}");
            }
        }
        self.stats.reset();
    }

    /// Return the number of currently cached icons in the hot tier.
    pub fn len(&self) -> usize {
        let cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.len()
    }

    /// Check whether the hot tier is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the total bytes currently stored in the hot tier.
    pub fn total_bytes(&self) -> usize {
        let total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total
    }

    /// Snapshot of hit/miss/eviction counters.
    pub fn stats(&self) -> IconCacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Resize the hot-tier capacity.
    ///
    /// If the new capacity is smaller, excess entries are evicted (LRU
    /// order). Warm-tier copies of evicted entries are preserved.
    pub fn resize(&self, new_capacity: usize) {
        let cap = NonZeroUsize::new(new_capacity.max(1)).expect("capacity must be > 0");
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.resize(cap);
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total = cache.iter().map(|(_, v)| v.len()).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_cache_is_empty() {
        let cache = IconCache::new(10);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let cache = IconCache::new(10);
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn put_and_get_returns_data() {
        let cache = IconCache::new(10);
        let data = vec![0x89, 0x50, 0x4E, 0x47];
        cache.put("hash1".to_string(), data.clone());
        let result = cache.get("hash1");
        assert_eq!(
            result.as_deref().map(|v| v.as_slice()),
            Some(data.as_slice())
        );
    }

    #[test]
    fn get_bytes_returns_owned_copy() {
        let cache = IconCache::new(10);
        cache.put("k".to_string(), vec![1, 2, 3]);
        assert_eq!(cache.get_bytes("k"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn contains_returns_correct_state() {
        let cache = IconCache::new(10);
        assert!(!cache.contains("key1"));
        cache.put("key1".to_string(), vec![1, 2, 3]);
        assert!(cache.contains("key1"));
    }

    #[test]
    fn clear_removes_all_entries() {
        let cache = IconCache::new(10);
        cache.put("a".to_string(), vec![1]);
        cache.put("b".to_string(), vec![2]);
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_none());
    }

    #[test]
    fn lru_eviction_works() {
        let cache = IconCache::new(2);
        cache.put("first".to_string(), vec![1]);
        cache.put("second".to_string(), vec![2]);
        cache.put("third".to_string(), vec![3]);

        assert_eq!(cache.len(), 2);
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
        assert!(cache.get("third").is_some());
    }

    #[test]
    fn minimum_capacity_is_one() {
        let cache = IconCache::new(0);
        cache.put("only".to_string(), vec![1]);
        assert_eq!(cache.len(), 1);
        cache.put("second".to_string(), vec![2]);
        assert_eq!(cache.len(), 1);
        assert!(cache.get("only").is_none());
        assert!(cache.get("second").is_some());
    }

    #[test]
    fn total_bytes_tracked_on_put_and_remove() {
        let cache = IconCache::new(10);
        assert_eq!(cache.total_bytes(), 0);

        cache.put("a".to_string(), vec![0; 100]);
        assert_eq!(cache.total_bytes(), 100);

        cache.put("b".to_string(), vec![0; 200]);
        assert_eq!(cache.total_bytes(), 300);

        cache.remove("a");
        assert_eq!(cache.total_bytes(), 200);

        cache.clear();
        assert_eq!(cache.total_bytes(), 0);
    }

    #[test]
    fn total_bytes_eviction_under_budget() {
        let cache = IconCache::new(1000);
        let big = vec![0u8; 16 * 1024 * 1024];
        cache.put("a".to_string(), big.clone());
        cache.put("b".to_string(), big.clone());
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.total_bytes(), 32 * 1024 * 1024);

        cache.put("c".to_string(), big.clone());
        assert!(cache.total_bytes() <= super::MAX_TOTAL_BYTES);
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn replacing_existing_key_updates_bytes() {
        let cache = IconCache::new(10);
        cache.put("x".to_string(), vec![0; 100]);
        assert_eq!(cache.total_bytes(), 100);

        cache.put("x".to_string(), vec![0; 300]);
        assert_eq!(cache.total_bytes(), 300);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_hot_miss_falls_to_warm() {
        let dir = TempDir::new().unwrap();
        let cache = IconCache::with_warm_dir(2, dir.path().to_path_buf());
        cache.put("abc12345".to_string(), vec![7, 8, 9]);
        // Force hot eviction by filling capacity.
        cache.put("def12345".to_string(), vec![0; 10]);
        cache.put("ghi12345".to_string(), vec![0; 10]);
        // "abc12345" evicted from hot, but still in warm.
        assert!(!cache.contains("abc12345"));
        assert!(cache.contains_any_tier("abc12345"));
        let got = cache.get("abc12345").unwrap();
        assert_eq!(got.as_ref().as_slice(), &[7, 8, 9]);
    }

    #[test]
    fn cache_evict_keeps_warm_file() {
        let dir = TempDir::new().unwrap();
        let cache = IconCache::with_warm_dir(1, dir.path().to_path_buf());
        cache.put("ab99887766".to_string(), vec![1, 2, 3]);
        // Force hot eviction.
        cache.put("cd99887766".to_string(), vec![4, 5, 6]);
        // Warm still has the first.
        let warm = dir.path().join("ab").join("ab99887766.png");
        assert!(
            warm.exists(),
            "warm file should persist across hot eviction"
        );
    }

    #[test]
    fn arc_payload_shared_across_getters() {
        let cache = IconCache::new(10);
        cache.put("shared".to_string(), vec![42; 16]);
        let a = cache.get("shared").unwrap();
        let b = cache.get("shared").unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn stats_track_hits_and_misses() {
        let cache = IconCache::new(5);
        cache.put("k".to_string(), vec![1]);
        let _ = cache.get("k");
        let _ = cache.get("missing");
        let snap = cache.stats();
        assert_eq!(snap.hot_hits, 1);
        assert_eq!(snap.misses, 1);
    }

    #[test]
    fn warm_hit_promotes_into_hot() {
        let dir = TempDir::new().unwrap();
        let cache = IconCache::with_warm_dir(2, dir.path().to_path_buf());
        cache.put("aa11223344".to_string(), vec![9, 9, 9]);
        // Evict from hot.
        cache.put("bb11223344".to_string(), vec![0; 8]);
        cache.put("cc11223344".to_string(), vec![0; 8]);
        assert!(!cache.contains("aa11223344"));
        // This get() should pull from warm and promote to hot.
        let _ = cache.get("aa11223344");
        assert!(cache.contains("aa11223344"));
    }
}
