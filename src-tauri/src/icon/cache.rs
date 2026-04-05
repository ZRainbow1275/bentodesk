use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// 32 MB total byte budget for cached icon data.
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;

/// Memory-bounded LRU cache for extracted icon PNGs.
///
/// Two eviction limits apply:
/// 1. Entry count — classic LRU capacity.
/// 2. Total bytes — the sum of all stored `Vec<u8>` payloads must not exceed
///    [`MAX_TOTAL_BYTES`]. When an insert would push the total over the limit,
///    the least-recently-used entries are evicted until the budget is satisfied.
pub struct IconCache {
    inner: Mutex<LruCache<String, Vec<u8>>>,
    /// Running total of bytes stored across all values.
    total_bytes: Mutex<usize>,
}

impl IconCache {
    /// Create a new icon cache with the given entry-count capacity.
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity must be > 0");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
            total_bytes: Mutex::new(0),
        }
    }

    /// Retrieve a cached icon by its hash key.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.get(key).cloned()
    }

    /// Insert an icon into the cache.
    ///
    /// If the new entry would push total cached bytes above [`MAX_TOTAL_BYTES`],
    /// the least-recently-used entries are evicted until the budget is met.
    pub fn put(&self, key: String, data: Vec<u8>) {
        let incoming_len = data.len();
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");

        // If the key already exists, subtract its old size first.
        if let Some(old) = cache.peek(&key) {
            *total = total.saturating_sub(old.len());
        }

        // Evict LRU entries until we have room (or the cache is empty).
        while *total + incoming_len > MAX_TOTAL_BYTES {
            if let Some((_evicted_key, evicted_val)) = cache.pop_lru() {
                *total = total.saturating_sub(evicted_val.len());
            } else {
                break;
            }
        }

        cache.put(key, data);
        *total += incoming_len;
    }

    /// Check whether a key is present in the cache.
    pub fn contains(&self, key: &str) -> bool {
        let cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.contains(key)
    }

    /// Remove a single entry from the cache by key.
    pub fn remove(&self, key: &str) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        if let Some(removed) = cache.pop(key) {
            let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
            *total = total.saturating_sub(removed.len());
        }
    }

    /// Clear all cached icons.
    pub fn clear(&self) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.clear();
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total = 0;
    }

    /// Return the number of currently cached icons.
    pub fn len(&self) -> usize {
        let cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.len()
    }

    /// Check whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the total bytes currently stored in the cache.
    pub fn total_bytes(&self) -> usize {
        let total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total
    }

    /// Resize the cache to a new capacity.
    ///
    /// If the new capacity is smaller, excess entries are evicted (LRU order).
    pub fn resize(&self, new_capacity: usize) {
        let cap = NonZeroUsize::new(new_capacity.max(1)).expect("capacity must be > 0");
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.resize(cap);
        // Recalculate total bytes after resize-eviction.
        let mut total = self.total_bytes.lock().expect("total_bytes lock poisoned");
        *total = cache.iter().map(|(_, v)| v.len()).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        cache.put("hash1".to_string(), data.clone());
        let result = cache.get("hash1");
        assert_eq!(result, Some(data));
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
        cache.put("third".to_string(), vec![3]); // Should evict "first"

        assert_eq!(cache.len(), 2);
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
        assert!(cache.get("third").is_some());
    }

    #[test]
    fn minimum_capacity_is_one() {
        let cache = IconCache::new(0); // Should clamp to 1
        cache.put("only".to_string(), vec![1]);
        assert_eq!(cache.len(), 1);
        cache.put("second".to_string(), vec![2]); // Evicts "only"
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
        // Use a small entry-count capacity but rely on byte budget eviction.
        let cache = IconCache::new(1000);

        // Each entry is 16 MB, budget is 32 MB → max 2 fit.
        let big = vec![0u8; 16 * 1024 * 1024];
        cache.put("a".to_string(), big.clone());
        cache.put("b".to_string(), big.clone());
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.total_bytes(), 32 * 1024 * 1024);

        // Third insert should evict "a" to stay within 32 MB.
        cache.put("c".to_string(), big.clone());
        assert!(cache.total_bytes() <= super::MAX_TOTAL_BYTES);
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn replacing_existing_key_updates_bytes() {
        let cache = IconCache::new(10);
        cache.put("x".to_string(), vec![0; 100]);
        assert_eq!(cache.total_bytes(), 100);

        // Replace with a larger value.
        cache.put("x".to_string(), vec![0; 300]);
        assert_eq!(cache.total_bytes(), 300);
        assert_eq!(cache.len(), 1);
    }
}
