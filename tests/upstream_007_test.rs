#[path = "../src/resolution/lru_cache.rs"]
mod lru_cache;
use lru_cache::LruCache;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
static COMPARISONS: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Debug)]
struct Key(usize);
impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        COMPARISONS.fetch_add(1, Ordering::Relaxed);
        self.0 == other.0
    }
}
impl Eq for Key {}
#[test]
fn case_007_hit_cost() {
    for capacity in [32, 512, 4096] {
        let mut cache = LruCache::new(capacity);
        for i in 0..capacity {
            cache.set(Key(i), i);
        }
        COMPARISONS.store(0, Ordering::Relaxed);
        for i in 0..1000 {
            assert_eq!(cache.get(&Key(i % capacity)), Some(i % capacity));
        }
        let cost = COMPARISONS.load(Ordering::Relaxed);
        eprintln!("capacity={capacity} hit comparisons={cost}");
        assert!(cost < 8000, "hit cost scales with capacity: {cost}");
    }
}
#[test]
fn case_007_order_and_boundaries() {
    for capacity in [1, 2, 17] {
        let mut cache = LruCache::new(capacity);
        let mut model = Vec::new();
        let mut seed = 7u64;
        for step in 0..50000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = (seed >> 32) as usize % 31;
            match seed % 19 {
                0 => {
                    cache.clear();
                    model.clear();
                }
                1..=9 => {
                    let expected = model.iter().position(|&(k, _)| k == key).map(|p| {
                        let item = model.remove(p);
                        model.push(item);
                        item.1
                    });
                    assert_eq!(cache.get(&key), expected);
                }
                _ => {
                    if let Some(p) = model.iter().position(|&(k, _)| k == key) {
                        model.remove(p);
                    } else if model.len() == capacity {
                        model.remove(0);
                    }
                    model.push((key, step));
                    cache.set(key, step);
                }
            }
            assert_eq!(cache.size(), model.len());
            for k in 0..31 {
                assert_eq!(cache.has(&k), model.iter().any(|&(x, _)| x == k));
            }
        }
    }
}
#[test]
#[should_panic]
fn case_007_zero_capacity() {
    let _ = LruCache::<usize, usize>::new(0);
}
