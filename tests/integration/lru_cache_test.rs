//! Rust port of `__tests__/integration/lru-cache.test.ts`.

use rustcodegraph::resolution::lru_cache::LruCache;

fn key(value: &str) -> String {
    value.to_owned()
}

mod lru_cache {
    use super::*;

    #[test]
    fn enforces_capacity_by_evicting_the_oldest_entry_on_overflow() {
        let mut cache = LruCache::<String, i32>::new(3);
        cache.set(key("a"), 1);
        cache.set(key("b"), 2);
        cache.set(key("c"), 3);
        cache.set(key("d"), 4); // evicts 'a'

        assert_eq!(cache.size(), 3);
        assert!(!cache.has(&key("a")));
        assert_eq!(cache.get(&key("a")), None);
        assert_eq!(cache.get(&key("b")), Some(2));
        assert_eq!(cache.get(&key("c")), Some(3));
        assert_eq!(cache.get(&key("d")), Some(4));
    }

    #[test]
    fn promotes_touched_keys_to_most_recent_so_they_survive_eviction() {
        let mut cache = LruCache::<String, i32>::new(3);
        cache.set(key("a"), 1);
        cache.set(key("b"), 2);
        cache.set(key("c"), 3);

        // Touch 'a' - it should now be most-recent.
        assert_eq!(cache.get(&key("a")), Some(1));

        cache.set(key("d"), 4); // evicts the LRU, which is now 'b' (not 'a')

        assert!(cache.has(&key("a")));
        assert!(!cache.has(&key("b")));
        assert!(cache.has(&key("c")));
        assert!(cache.has(&key("d")));
    }

    #[test]
    fn overwriting_an_existing_key_refreshes_its_recency_but_does_not_grow_size() {
        let mut cache = LruCache::<String, i32>::new(2);
        cache.set(key("a"), 1);
        cache.set(key("b"), 2);
        cache.set(key("a"), 99); // 'a' is now most-recent

        assert_eq!(cache.size(), 2);
        assert_eq!(cache.get(&key("a")), Some(99));

        cache.set(key("c"), 3); // should evict 'b', not 'a'

        assert!(cache.has(&key("a")));
        assert!(!cache.has(&key("b")));
        assert!(cache.has(&key("c")));
    }

    #[test]
    fn stores_null_values_used_by_the_file_content_cache() {
        let mut cache = LruCache::<String, Option<String>>::new(2);
        cache.set(key("missing.ts"), None);

        assert!(cache.has(&key("missing.ts")));
        assert_eq!(cache.get(&key("missing.ts")), Some(None));
    }

    #[test]
    fn clear_resets_the_cache() {
        let mut cache = LruCache::<String, i32>::new(3);
        cache.set(key("a"), 1);
        cache.set(key("b"), 2);
        cache.clear();

        assert_eq!(cache.size(), 0);
        assert!(!cache.has(&key("a")));
    }

    #[test]
    fn rejects_non_positive_capacity() {
        assert!(std::panic::catch_unwind(|| LruCache::<String, i32>::new(0)).is_err());

        // TypeScript also asserts `new LRUCache(-1)` and `new LRUCache(NaN)`
        // throw. Rust's constructor accepts `usize`, so those invalid inputs are
        // rejected before they can reach `LruCache::new`.
        assert!(usize::try_from(-1_i32).is_err());
        assert!(f64::NAN.partial_cmp(&0.0).is_none());
    }

    #[test]
    fn stays_bounded_under_heavy_churn_regression_for_oom_scenario() {
        let mut cache = LruCache::<String, i32>::new(100);

        for i in 0..10_000 {
            cache.set(format!("key{i}"), i);
        }

        assert_eq!(cache.size(), 100);
        // The last 100 keys should still be present, the rest evicted.
        assert!(cache.has(&key("key9999")));
        assert!(cache.has(&key("key9900")));
        assert!(!cache.has(&key("key0")));
    }
}
