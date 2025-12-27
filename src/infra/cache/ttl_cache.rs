use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

struct CacheEntry<V> {
    value: V,
    created_at: Instant,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        Self {
            value,
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }

    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

pub struct TtlCache<K, V> {
    inner: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    ttl: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        let cache = self.inner.read().await;
        cache.get(key).and_then(|entry| {
            if entry.is_expired(self.ttl) {
                None
            } else {
                Some(entry.value.clone())
            }
        })
    }

    pub async fn set(&self, key: K, value: V) {
        let mut cache = self.inner.write().await;
        cache.insert(key, CacheEntry::new(value));
    }

    pub async fn invalidate(&self, key: &K) {
        let mut cache = self.inner.write().await;
        cache.remove(key);
    }

    pub async fn clear(&self) {
        let mut cache = self.inner.write().await;
        cache.clear();
    }

    pub async fn age(&self, key: &K) -> Option<Duration> {
        let cache = self.inner.read().await;
        cache.get(key).map(|entry| entry.age())
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

impl<K, V> Clone for TtlCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            ttl: self.ttl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_get_set() {
        let cache = TtlCache::new(60);
        cache.set("key1".to_string(), "value1".to_string()).await;

        let result = cache.get(&"key1".to_string()).await;
        assert_eq!(result, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = TtlCache::new(60);
        cache.set("key1".to_string(), "value1".to_string()).await;
        cache.invalidate(&"key1".to_string()).await;

        let result = cache.get(&"key1".to_string()).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache: TtlCache<String, String> = TtlCache::new(60);
        let result = cache.get(&"nonexistent".to_string()).await;
        assert_eq!(result, None);
    }
}
