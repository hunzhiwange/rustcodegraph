//! Small insertion-ordered LRU cache.
//!
//! Rust `HashMap` does not preserve insertion order, so we track recency in a
//! `VecDeque`. The behavior matches the TypeScript `Map` implementation:
//! `get` refreshes an entry, and `set` evicts the oldest entry when full.
//!
//! 中文维护提示：解析阶段大量重复读取同名节点、文件内容和 import 映射；这个实现
//! 刻意保持依赖面很小，容量满时按最近访问顺序淘汰最旧 key。

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct LruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    max: usize,
    store: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(max: usize) -> Self {
        assert!(max > 0, "LRUCache max must be a positive finite number");
        Self {
            max,
            store: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn size(&self) -> usize {
        self.store.len()
    }

    pub fn has(&self, key: &K) -> bool {
        self.store.contains_key(key)
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        // 取值时也要提升到队尾，保证“最近使用”不仅包含写入，也包含命中读取。
        let value = self.store.get(key).cloned()?;
        self.touch(key);
        Some(value)
    }

    pub fn set(&mut self, key: K, value: V) {
        if self.store.contains_key(&key) {
            // 先删旧 key 再 push，可以避免队列里累积重复 key；淘汰阶段只需要看队首。
            self.order.retain(|k| k != &key);
        } else {
            while self.store.len() >= self.max {
                if let Some(oldest) = self.order.pop_front() {
                    self.store.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.order.push_back(key.clone());
        self.store.insert(key, value);
    }

    pub fn clear(&mut self) {
        self.store.clear();
        self.order.clear();
    }

    fn touch(&mut self, key: &K) {
        // VecDeque 容量很小且只在解析批次内使用，线性 retain 的简单实现比引入
        // 额外链表/索引结构更稳。
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
    }
}
