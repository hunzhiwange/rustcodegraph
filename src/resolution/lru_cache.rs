//! Bounded LRU cache with recency links stored as stable entry indices.
//! Hits update only neighboring links; evictions reuse the oldest entry slot.

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
struct Entry<K, V> {
    key: K,
    value: V,
    older: Option<usize>,
    newer: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct LruCache<K: Eq + Hash + Clone, V: Clone> {
    max: usize,
    store: HashMap<K, usize>,
    entries: Vec<Entry<K, V>>,
    oldest: Option<usize>,
    newest: Option<usize>,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    pub fn new(max: usize) -> Self {
        assert!(max > 0, "LRUCache max must be a positive finite number");
        Self {
            max,
            store: HashMap::new(),
            entries: Vec::new(),
            oldest: None,
            newest: None,
        }
    }
    pub fn size(&self) -> usize {
        self.store.len()
    }
    pub fn has(&self, key: &K) -> bool {
        self.store.contains_key(key)
    }
    pub fn get(&mut self, key: &K) -> Option<V> {
        let index = *self.store.get(key)?;
        self.touch(index);
        Some(self.entries[index].value.clone())
    }
    pub fn set(&mut self, key: K, value: V) {
        if let Some(&index) = self.store.get(&key) {
            self.entries[index].value = value;
            self.touch(index);
            return;
        }
        let entry = Entry {
            key: key.clone(),
            value,
            older: self.newest,
            newer: None,
        };
        let index = if self.store.len() == self.max {
            let index = self.oldest.expect("full cache has oldest entry");
            self.unlink(index);
            self.store.remove(&self.entries[index].key);
            self.entries[index] = Entry {
                older: self.newest,
                ..entry
            };
            index
        } else {
            self.entries.push(entry);
            self.entries.len() - 1
        };
        self.append(index);
        self.store.insert(key, index);
    }
    pub fn clear(&mut self) {
        self.store.clear();
        self.entries.clear();
        self.oldest = None;
        self.newest = None;
    }
    fn unlink(&mut self, index: usize) {
        let (older, newer) = (self.entries[index].older, self.entries[index].newer);
        if let Some(i) = older {
            self.entries[i].newer = newer;
        } else {
            self.oldest = newer;
        }
        if let Some(i) = newer {
            self.entries[i].older = older;
        } else {
            self.newest = older;
        }
    }
    fn append(&mut self, index: usize) {
        self.entries[index].older = self.newest;
        self.entries[index].newer = None;
        if let Some(i) = self.newest {
            self.entries[i].newer = Some(index);
        } else {
            self.oldest = Some(index);
        }
        self.newest = Some(index);
    }
    fn touch(&mut self, index: usize) {
        if self.newest == Some(index) {
            return;
        }
        self.unlink(index);
        self.append(index);
    }
}
