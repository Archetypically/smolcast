//! A bounded cache of values a worker produces on request, such as the row
//! icon, emoji glyph and clipboard thumbnail textures.
//!
//! Values are rendered at one pixel edge. Changing the edge (a monitor DPI
//! change) starts the cache over, and a result for any other edge is refused,
//! so a request in flight across the change is never cached at the old size.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

pub struct RequestCache<K, V> {
    values: HashMap<K, Slot<V>>,
    pending: HashSet<K>,
    failed: HashSet<K>,
    edge: u32,
    cap: usize,
    /// Advances on every insert and hit; the smallest `used` is the least
    /// recently used.
    clock: u64,
}

struct Slot<V> {
    value: V,
    /// The `clock` value at the last insert or hit.
    used: u64,
}

impl<K: Eq + Hash + Clone, V: Clone> RequestCache<K, V> {
    /// A cache holding at most `cap` values.
    pub fn new(cap: usize) -> Self {
        Self {
            values: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
            edge: 0,
            cap,
            clock: 0,
        }
    }

    /// The pixel edge values are requested and cached at.
    pub fn edge(&self) -> u32 {
        self.edge
    }

    /// Values are prescaled to exact pixels, so a new edge starts over.
    pub fn set_edge(&mut self, edge: u32) {
        if edge != self.edge {
            self.clear();
            self.edge = edge;
        }
    }

    /// The cached value for `key`, now the most recently used.
    pub fn get(&mut self, key: &K) -> Option<V> {
        let slot = self.values.get_mut(key)?;
        self.clock += 1;
        slot.used = self.clock;
        Some(slot.value.clone())
    }

    /// True when `key` should be requested now: it is not cached, in flight,
    /// or failed. It is then marked in flight.
    pub fn begin(&mut self, key: &K) -> bool {
        if self.values.contains_key(key) || self.failed.contains(key) {
            return false;
        }
        self.pending.insert(key.clone())
    }

    /// Withdraw an in-flight request rendered at `edge` so the next `begin`
    /// asks again. A request at another edge stays in flight.
    pub fn cancel(&mut self, key: &K, edge: u32) {
        if edge == self.edge {
            self.pending.remove(key);
        }
    }

    /// A result for `key` rendered at `edge` arrived. True when it answers a
    /// request still in flight at the current edge; the caller then stores it
    /// with `insert` or `fail`. Otherwise it is stale and must be dropped.
    pub fn accept(&mut self, key: &K, edge: u32) -> bool {
        edge == self.edge && self.pending.remove(key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.pending.remove(&key);
        self.failed.remove(&key);
        self.clock += 1;
        self.values.insert(
            key,
            Slot {
                value,
                used: self.clock,
            },
        );
        // One insert adds at most one value, so one eviction restores the cap.
        if self.values.len() > self.cap {
            let oldest = self
                .values
                .iter()
                .min_by_key(|(_, slot)| slot.used)
                .map(|(k, _)| k.clone());
            if let Some(old) = oldest {
                self.values.remove(&old);
            }
        }
    }

    /// `key` could not be produced; it is not requested again until the
    /// cache starts over.
    pub fn fail(&mut self, key: K) {
        self.pending.remove(&key);
        self.failed.insert(key);
    }

    /// Let failed keys be requested again.
    pub fn retry_failed(&mut self) {
        self.failed.clear();
    }

    /// Forget `key` entirely: cached, in flight or failed.
    pub fn remove(&mut self, key: &K) {
        self.retain(|k| k != key);
    }

    /// Keep only the keys `keep` accepts, in every state.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.values.retain(|k, _| keep(k));
        self.pending.retain(|k| keep(k));
        self.failed.retain(|k| keep(k));
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.pending.clear();
        self.failed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(cap: usize) -> RequestCache<u32, &'static str> {
        let mut cache = RequestCache::new(cap);
        cache.set_edge(32);
        cache
    }

    #[test]
    fn begin_requests_each_key_once() {
        let mut c = cache(4);
        assert!(c.begin(&1));
        assert!(!c.begin(&1));
        assert!(c.accept(&1, 32));
        c.insert(1, "a");
        assert!(!c.begin(&1));
        assert_eq!(c.get(&1), Some("a"));
    }

    #[test]
    fn failed_keys_are_not_requested_again() {
        let mut c = cache(4);
        assert!(c.begin(&1));
        assert!(c.accept(&1, 32));
        c.fail(1);
        assert!(!c.begin(&1));
        c.set_edge(48);
        assert!(c.begin(&1));
    }

    #[test]
    fn a_result_for_another_edge_is_refused() {
        let mut c = cache(4);
        assert!(c.begin(&1));
        c.set_edge(48);
        assert!(c.begin(&1));
        // The request sent at 32 px arrives after the new one went out.
        assert!(!c.accept(&1, 32));
        assert!(c.accept(&1, 48));
    }

    #[test]
    fn a_result_nobody_waits_for_is_refused() {
        let mut c = cache(4);
        assert!(!c.accept(&1, 32));
        assert!(c.begin(&1));
        c.cancel(&1, 48);
        assert!(!c.begin(&1));
        c.cancel(&1, 32);
        assert!(!c.accept(&1, 32));
        assert!(c.begin(&1));
    }

    #[test]
    fn evicts_the_least_recently_used() {
        let mut c = cache(2);
        c.insert(1, "a");
        c.insert(2, "b");
        assert_eq!(c.get(&1), Some("a"));
        c.insert(3, "c");
        assert_eq!(c.get(&2), None);
        assert_eq!(c.get(&1), Some("a"));
        assert_eq!(c.get(&3), Some("c"));
    }

    #[test]
    fn reinserting_does_not_duplicate_recency() {
        let mut c = cache(2);
        c.insert(1, "a");
        c.insert(1, "b");
        c.insert(2, "c");
        assert_eq!(c.get(&1), Some("b"));
        assert_eq!(c.get(&2), Some("c"));
    }

    #[test]
    fn retain_and_remove_cover_every_state() {
        let mut c = cache(4);
        c.insert(1, "a");
        assert!(c.begin(&2));
        c.fail(3);
        c.retain(|k| *k == 1);
        assert!(c.begin(&2));
        assert!(c.begin(&3));
        c.remove(&1);
        assert_eq!(c.get(&1), None);
        assert!(c.begin(&1));
    }
}
