use lru::LruCache;
use std::hash::Hash;
use std::num::NonZeroUsize;

pub struct BoundedLruCache<K, V> {
    inner: LruCache<K, V>,
}

impl<K: Eq + Hash, V> BoundedLruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).expect("capacity must be > 0");
        Self {
            inner: LruCache::new(cap),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.inner.put(key, value);
    }

    pub fn contains(&self, key: &K) -> bool {
        self.inner.peek(key).is_some()
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
    }

    #[test]
    fn test_contains() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a", 1);

        assert!(cache.contains(&"a"));
        assert!(!cache.contains(&"b"));
    }

    #[test]
    fn test_eviction_on_capacity() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);

        assert!(!cache.contains(&"a"));
        assert!(cache.contains(&"b"));
        assert!(cache.contains(&"c"));
    }

    #[test]
    fn test_get_updates_lru_order() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);

        // Access "a" to make it recently used
        let _ = cache.get(&"a");

        // Insert "c", should evict "b" (LRU)
        cache.insert("c", 3);

        assert!(cache.contains(&"a"));
        assert!(!cache.contains(&"b"));
        assert!(cache.contains(&"c"));
    }

    #[test]
    fn test_clear() {
        let mut cache = BoundedLruCache::new(2);
        cache.insert("a", 1);
        cache.insert("b", 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(!cache.contains(&"a"));
    }

    #[test]
    fn test_iter() {
        let mut cache = BoundedLruCache::new(3);
        cache.insert("a", 1);
        cache.insert("b", 2);
        cache.insert("c", 3);

        let items: Vec<_> = cache.iter().collect();
        assert_eq!(items.len(), 3);
    }
}
