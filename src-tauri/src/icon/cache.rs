use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Memory-bounded LRU cache for extracted icon PNGs.
pub struct IconCache {
    inner: Mutex<LruCache<String, Vec<u8>>>,
}

impl IconCache {
    /// Create a new icon cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity must be > 0");
        Self {
            inner: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Retrieve a cached icon by its hash key.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.get(key).cloned()
    }

    /// Insert an icon into the cache.
    pub fn put(&self, key: String, data: Vec<u8>) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.put(key, data);
    }

    /// Check whether a key is present in the cache.
    pub fn contains(&self, key: &str) -> bool {
        let cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.contains(key)
    }

    /// Remove a single entry from the cache by key.
    pub fn remove(&self, key: &str) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.pop(key);
    }

    /// Clear all cached icons.
    pub fn clear(&self) {
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.clear();
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

    /// Resize the cache to a new capacity.
    ///
    /// If the new capacity is smaller, excess entries are evicted (LRU order).
    pub fn resize(&self, new_capacity: usize) {
        let cap = NonZeroUsize::new(new_capacity.max(1)).expect("capacity must be > 0");
        let mut cache = self.inner.lock().expect("icon cache lock poisoned");
        cache.resize(cap);
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
}
