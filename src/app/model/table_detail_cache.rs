use std::num::NonZeroUsize;

use lru::LruCache;

use crate::domain::Table;

const MIN_CAPACITY: usize = 500;
const MAX_CAPACITY: usize = 10_000;

pub struct TableDetailCache {
    tables: LruCache<String, Table>,
}

impl Default for TableDetailCache {
    fn default() -> Self {
        Self::with_capacity(MIN_CAPACITY)
    }
}

impl TableDetailCache {
    pub fn insert(&mut self, qualified_name: String, table: Table) {
        let _ = self.tables.put(qualified_name, table);
    }

    pub fn contains(&self, qualified_name: &str) -> bool {
        self.tables.contains(qualified_name)
    }

    pub fn peek(&self, qualified_name: &str) -> Option<&Table> {
        self.tables.peek(qualified_name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Table)> {
        self.tables.iter()
    }

    pub fn evict(&mut self, qualified_names: &[String]) {
        for qualified_name in qualified_names {
            let _ = self.tables.pop(qualified_name);
        }
    }

    pub fn clear(&mut self) {
        self.tables.clear();
    }

    pub fn resize_for_table_count(&mut self, table_count: usize) {
        self.resize(table_count.clamp(MIN_CAPACITY, MAX_CAPACITY));
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            tables: LruCache::new(NonZeroUsize::new(capacity).expect("capacity must be > 0")),
        }
    }

    fn resize(&mut self, capacity: usize) {
        self.tables
            .resize(NonZeroUsize::new(capacity).expect("capacity must be > 0"));
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_support::table::minimal;

    fn table(name: &str) -> Table {
        minimal("public", name)
    }

    fn cache_with(capacity: usize, names: &[&str]) -> TableDetailCache {
        let mut cache = TableDetailCache::with_capacity(capacity);
        for name in names {
            cache.insert(format!("public.{name}"), table(name));
        }
        cache
    }

    fn cached_names(cache: &TableDetailCache) -> Vec<String> {
        let mut names: Vec<String> = cache.iter().map(|(name, _)| name.clone()).collect();
        names.sort();
        names
    }

    #[test]
    fn contains_reports_only_inserted_tables() {
        let cache = cache_with(3, &["users"]);

        assert!(cache.contains("public.users"));
        assert!(!cache.contains("public.orders"));
    }

    #[test]
    fn evict_removes_specified_entries_and_ignores_missing_keys() {
        let mut cache = cache_with(3, &["users", "orders", "items"]);

        cache.evict(&[
            "public.users".to_string(),
            "public.orders".to_string(),
            "public.nonexistent".to_string(),
        ]);

        assert_eq!(cached_names(&cache), ["public.items"]);
    }

    #[test]
    fn clear_removes_all_tables() {
        let mut cache = cache_with(3, &["t1", "t2"]);

        cache.clear();

        assert_eq!(cache.iter().count(), 0);
    }

    #[test]
    fn insert_beyond_capacity_evicts_least_recently_inserted() {
        let cache = cache_with(2, &["t1", "t2", "t3"]);

        assert_eq!(cached_names(&cache), ["public.t2", "public.t3"]);
    }

    #[test]
    fn reads_do_not_promote_tables_before_eviction() {
        let mut cache = cache_with(2, &["t1", "t2"]);

        assert!(cache.peek("public.t1").is_some());
        assert!(cache.contains("public.t1"));
        assert_eq!(cache.iter().count(), 2);
        cache.insert("public.t3".to_string(), table("t3"));

        assert_eq!(cached_names(&cache), ["public.t2", "public.t3"]);
    }

    #[test]
    fn expanding_capacity_preserves_entries() {
        let mut cache = cache_with(2, &["t1", "t2"]);

        cache.resize(5);
        for name in ["t3", "t4", "t5", "t6"] {
            cache.insert(format!("public.{name}"), table(name));
        }

        assert_eq!(
            cached_names(&cache),
            [
                "public.t2",
                "public.t3",
                "public.t4",
                "public.t5",
                "public.t6"
            ]
        );
    }

    #[test]
    fn shrinking_capacity_evicts_least_recently_used_entries() {
        let mut cache = cache_with(3, &["t1", "t2", "t3"]);

        cache.resize(2);
        let after_shrink = cached_names(&cache);
        cache.insert("public.t4".to_string(), table("t4"));

        assert_eq!(after_shrink, ["public.t2", "public.t3"]);
        assert_eq!(cached_names(&cache), ["public.t3", "public.t4"]);
    }

    #[rstest]
    #[case(0, MIN_CAPACITY)]
    #[case(560, 560)]
    #[case(20_000, MAX_CAPACITY)]
    fn table_count_capacity_is_clamped(#[case] table_count: usize, #[case] expected: usize) {
        let mut cache = TableDetailCache::default();

        cache.resize_for_table_count(table_count);

        assert_eq!(cache.tables.cap().get(), expected);
    }
}
