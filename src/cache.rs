// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Cache Module
//!
//! Generic time-bounded cache used by the engine for `render_page`
//! results. Two policies stack:
//!
//! - **TTL expiration** — every entry carries a per-insert deadline.
//!   `Cache::get` returns `None` once the deadline has passed; call
//!   `Cache::remove_expired` periodically to reclaim memory.
//! - **LRU eviction** — when constructed with `Cache::with_capacity`,
//!   the cache enforces a hard upper bound. Inserts that would exceed
//!   the cap evict the least-recently-used entry. `Cache::get` bumps
//!   access recency, so frequently-read entries stay hot.
//!
//! The type is generic over both key and value (`Cache<K, V>`); the
//! engine instantiates it as `Cache<String, String>` to cache
//! rendered page bodies.
//!
//! # Examples
//!
//! ```
//! use staticweaver::cache::Cache;
//! use std::time::Duration;
//!
//! let mut cache: Cache<String, u32> =
//!     Cache::with_capacity(Duration::from_secs(60), 2);
//! let _ = cache.insert("a".to_string(), 1);
//! let _ = cache.insert("b".to_string(), 2);
//! // Touching `a` promotes it; the next insert evicts `b`, not `a`.
//! let _ = cache.get(&"a".to_string());
//! let _ = cache.insert("c".to_string(), 3);
//! assert!(cache.contains_key(&"a".to_string()));
//! assert!(!cache.contains_key(&"b".to_string()));
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// Snapshot of cache counters at a point in time. Returned by
/// [`Cache::stats`].
///
/// All fields are monotonically increasing `u64`s — they grow as the cache
/// is used and never decrement. Take two snapshots and subtract to get a
/// rate / per-window count. Designed for cheap export to `prometheus`,
/// `metrics`, or `opentelemetry` without forcing a dep choice on the
/// caller. Issue #40.
///
/// # Examples
///
/// ```
/// use staticweaver::cache::Cache;
/// use std::time::Duration;
///
/// let mut cache: Cache<String, u32> = Cache::new(Duration::from_secs(60));
/// let _ = cache.insert("a".to_string(), 1);
/// let _ = cache.get(&"a".to_string());   // hit
/// let _ = cache.get(&"b".to_string());   // miss
///
/// let stats = cache.stats();
/// assert_eq!(stats.inserts, 1);
/// assert_eq!(stats.hits, 1);
/// assert_eq!(stats.misses, 1);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Number of `insert` calls (new entries + updates).
    pub inserts: u64,
    /// Number of `get` calls that returned `Some` for a live entry.
    pub hits: u64,
    /// Number of `get` calls that returned `None` (absent key, no hit).
    pub misses: u64,
    /// Number of entries removed because they hit the capacity ceiling
    /// (LRU evictions). Does **not** count TTL expirations — see
    /// `ttl_expired` for those.
    pub evictions: u64,
    /// Number of entries reaped by `get` finding them expired or by
    /// `remove_expired` sweeping them.
    pub ttl_expired: u64,
}

/// Represents a cached item with its value, expiration time, and the
/// monotonic counter value at the entry's most recent access (used for
/// LRU eviction when a capacity bound is hit).
#[derive(Debug, Clone)]
struct CachedItem<T> {
    value: T,
    expiration: Instant,
    last_access: u64,
}

/// A simple cache implementation with expiration and optional capacity limit.
///
/// This cache provides time-based expiration for items and an optional maximum capacity.
/// It's designed to be generic over both key and value types for maximum flexibility.
///
/// # Examples
///
/// ```
/// use staticweaver::cache::Cache;
/// use std::time::Duration;
///
/// let mut cache: Cache<String, u32> = Cache::new(Duration::from_secs(60));
/// let _ = cache.insert("visits".to_string(), 1);
/// assert_eq!(cache.get(&"visits".to_string()), Some(&1));
/// ```
#[derive(Debug, Clone)]
pub struct Cache<K, V> {
    items: HashMap<K, CachedItem<V>>,
    ttl: Duration,
    capacity: Option<usize>,
    /// Monotonic counter bumped on every `get` / `insert` / `update` /
    /// `refresh` hit. The `CachedItem::last_access` field copies the
    /// counter's value at that moment, producing a total ordering on
    /// usage recency without allocating a secondary index.
    access_counter: u64,
    /// Cumulative hit / miss / eviction / ttl-expiry counters. Exposed
    /// via [`Cache::stats`]. Issue #40.
    stats: CacheStats,
}

impl<K: Hash + Eq + Clone, V: Clone> Cache<K, V> {
    /// Creates a new Cache with the specified time-to-live (TTL) for items.
    ///
    /// # Arguments
    ///
    /// * `ttl` - The time-to-live for cached items.
    ///
    /// # Panics
    ///
    /// Panics if `ttl` is zero.
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let cache: Cache<String, String> = Cache::new(Duration::from_secs(60));
    /// ```
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        assert!(!ttl.is_zero(), "TTL must be greater than zero");
        Self {
            items: HashMap::new(),
            ttl,
            capacity: None,
            access_counter: 0,
            stats: CacheStats::default(),
        }
    }

    /// Returns an iterator over the key-value pairs in the cache.
    ///
    /// Only entries that have not yet expired are yielded.
    ///
    /// # Returns
    ///
    /// An iterator over the key-value pairs in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("a".to_string(), "1".to_string());
    /// let _ = cache.insert("b".to_string(), "2".to_string());
    /// assert_eq!(cache.iter().count(), 2);
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        let now = Instant::now();
        self.items.iter().filter_map(move |(k, item)| {
            if item.expiration > now {
                Some((k, &item.value))
            } else {
                None
            }
        })
    }

    /// Creates a new Cache with the specified TTL and initial capacity.
    ///
    /// # Arguments
    ///
    /// * `ttl` - The time-to-live for cached items.
    /// * `capacity` - The initial capacity of the cache.
    ///
    /// # Panics
    ///
    /// Panics if `ttl` is zero.
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let cache: Cache<String, String> = Cache::with_capacity(Duration::from_secs(60), 100);
    /// ```
    #[must_use]
    pub fn with_capacity(ttl: Duration, capacity: usize) -> Self {
        assert!(!ttl.is_zero(), "TTL must be greater than zero");
        Self {
            items: HashMap::with_capacity(capacity),
            ttl,
            capacity: Some(capacity),
            access_counter: 0,
            stats: CacheStats::default(),
        }
    }

    /// Returns a snapshot of cumulative cache counters (hits, misses,
    /// evictions, ttl-expirations, inserts). Issue #40.
    ///
    /// All counters are monotonically increasing `u64`s. Take two
    /// snapshots and subtract to get a per-window rate. Designed for
    /// cheap export to `prometheus`, `metrics`, or `opentelemetry`
    /// without forcing a dependency choice.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, u32> = Cache::new(Duration::from_secs(60));
    /// assert_eq!(cache.stats().inserts, 0);
    /// let _ = cache.insert("k".to_string(), 1);
    /// assert_eq!(cache.stats().inserts, 1);
    /// ```
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache is at capacity and the key doesn't already exist, the
    /// least-recently-used entry is evicted to make room.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to insert.
    /// * `value` - The value to insert.
    ///
    /// # Returns
    ///
    /// The old value if the key was already present.
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache = Cache::new(Duration::from_secs(60));
    /// cache.insert("key".to_string(), "value".to_string());
    /// ```
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        // If adding a *new* key would exceed capacity, evict the
        // least-recently-used entry first. Updating an existing key is
        // always allowed and doesn't trigger eviction. The loop handles
        // the case where the cap was lowered below the current size —
        // repeatedly drop the oldest entry until there's room for the
        // new key.
        if let Some(cap) = self.capacity {
            while self.items.len() >= cap
                && !self.items.contains_key(&key)
            {
                let victim = self
                    .items
                    .iter()
                    .min_by_key(|(_, item)| item.last_access)
                    .map(|(k, _)| k.clone());
                match victim {
                    Some(k) => {
                        let _ = self.items.remove(&k);
                        self.stats.evictions =
                            self.stats.evictions.wrapping_add(1);
                    }
                    None => break,
                }
            }
        }
        self.access_counter = self.access_counter.wrapping_add(1);
        let expiration = Instant::now() + self.ttl;
        self.stats.inserts = self.stats.inserts.wrapping_add(1);
        self.items
            .insert(
                key,
                CachedItem {
                    value,
                    expiration,
                    last_access: self.access_counter,
                },
            )
            .map(|old_item| old_item.value)
    }

    /// Retrieves a value from the cache if it exists and hasn't expired.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the value if it exists and hasn't expired, or `None` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache = Cache::new(Duration::from_secs(60));
    /// cache.insert("key".to_string(), "value".to_string());
    ///
    /// assert_eq!(cache.get(&"key".to_string()), Some(&"value".to_string()));
    /// ```
    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Promote the entry to most-recently-used on a live hit so the
        // LRU eviction policy actually tracks usage, not just insertion
        // order. Expired entries are not promoted; callers get `None`
        // and `remove_expired` will collect them on the next pass.
        let now = Instant::now();
        let next = self.access_counter.wrapping_add(1);
        let item = match self.items.get_mut(key) {
            Some(item) => item,
            None => {
                self.stats.misses = self.stats.misses.wrapping_add(1);
                return None;
            }
        };
        if item.expiration > now {
            item.last_access = next;
            self.access_counter = next;
            self.stats.hits = self.stats.hits.wrapping_add(1);
            Some(&item.value)
        } else {
            self.stats.ttl_expired =
                self.stats.ttl_expired.wrapping_add(1);
            self.stats.misses = self.stats.misses.wrapping_add(1);
            None
        }
    }

    /// Removes expired items from the cache.
    ///
    /// This method should be called periodically to clean up the cache.
    ///
    /// Time complexity: O(n) where n is the number of items in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("k".to_string(), "v".to_string());
    /// cache.remove_expired();
    /// assert_eq!(cache.len(), 1);
    /// ```
    pub fn remove_expired(&mut self) {
        let now = Instant::now();
        let before = self.items.len() as u64;
        self.items.retain(|_, item| item.expiration > now);
        let reaped = before.saturating_sub(self.items.len() as u64);
        self.stats.ttl_expired =
            self.stats.ttl_expired.wrapping_add(reaped);
    }

    /// Checks if a key exists in the cache and hasn't expired.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check.
    ///
    /// # Returns
    ///
    /// `true` if the key exists and hasn't expired, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, i32> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("n".to_string(), 1);
    /// assert!(cache.contains_key(&"n".to_string()));
    /// assert!(!cache.contains_key(&"missing".to_string()));
    /// ```
    pub fn contains_key(&self, key: &K) -> bool {
        self.items
            .get(key)
            .map_or(false, |item| item.expiration > Instant::now())
    }

    /// Gets the remaining time-to-live for an item.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check.
    ///
    /// # Returns
    ///
    /// An `Option` containing the remaining TTL if the item exists and hasn't expired, or `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, i32> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("n".to_string(), 1);
    /// assert!(cache.ttl(&"n".to_string()).is_some());
    /// assert!(cache.ttl(&"missing".to_string()).is_none());
    /// ```
    pub fn ttl(&self, key: &K) -> Option<Duration> {
        self.items.get(key).and_then(|item| {
            let now = Instant::now();
            if item.expiration > now {
                Some(item.expiration - now)
            } else {
                None
            }
        })
    }

    /// Refreshes the expiration time for an item.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the item to refresh.
    ///
    /// # Returns
    ///
    /// `true` if the item was found and refreshed, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, i32> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("n".to_string(), 1);
    /// assert!(cache.refresh(&"n".to_string()));
    /// assert!(!cache.refresh(&"missing".to_string()));
    /// ```
    pub fn refresh(&mut self, key: &K) -> bool {
        let next = self.access_counter.wrapping_add(1);
        if let Some(item) = self.items.get_mut(key) {
            item.expiration = Instant::now() + self.ttl;
            item.last_access = next;
            self.access_counter = next;
            true
        } else {
            false
        }
    }

    /// Removes a key-value pair from the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to remove.
    ///
    /// # Returns
    ///
    /// The removed value if the key was present, or `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("k".to_string(), "v".to_string());
    /// assert_eq!(cache.remove(&"k".to_string()), Some("v".to_string()));
    /// assert_eq!(cache.remove(&"k".to_string()), None);
    /// ```
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.items.remove(key).map(|item| item.value)
    }

    /// Updates the value for an existing key in the cache.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to update.
    /// * `value` - The new value to set.
    ///
    /// # Returns
    ///
    /// `true` if the key was found and updated, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("k".to_string(), "old".to_string());
    /// assert!(cache.update(&"k".to_string(), "new".to_string()));
    /// assert_eq!(cache.get(&"k".to_string()), Some(&"new".to_string()));
    /// ```
    pub fn update(&mut self, key: &K, value: V) -> bool {
        let next = self.access_counter.wrapping_add(1);
        if let Some(item) = self.items.get_mut(key) {
            item.value = value;
            item.expiration = Instant::now() + self.ttl;
            item.last_access = next;
            self.access_counter = next;
            true
        } else {
            false
        }
    }

    /// Sets a maximum capacity for the cache.
    ///
    /// If the cache is already larger than this capacity, it will not
    /// remove items immediately, but it will prevent new items from being
    /// added until it's below capacity (via LRU eviction on `insert`).
    ///
    /// # Arguments
    ///
    /// * `capacity` - The maximum number of items the cache can hold.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// cache.set_capacity(128);
    /// ```
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = Some(capacity);
    }

    /// Clears all items from the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, i32> =
    ///     Cache::new(Duration::from_secs(60));
    /// let _ = cache.insert("n".to_string(), 1);
    /// cache.clear();
    /// assert!(cache.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Returns the number of items in the cache.
    ///
    /// # Returns
    ///
    /// The number of key-value pairs currently in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let mut cache: Cache<String, i32> =
    ///     Cache::new(Duration::from_secs(60));
    /// assert_eq!(cache.len(), 0);
    /// let _ = cache.insert("a".to_string(), 1);
    /// assert_eq!(cache.len(), 1);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Checks if the cache is empty.
    ///
    /// # Returns
    ///
    /// `true` if the cache is empty, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::cache::Cache;
    /// use std::time::Duration;
    ///
    /// let cache: Cache<String, String> =
    ///     Cache::new(Duration::from_secs(60));
    /// assert!(cache.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<K: Hash + Eq + Clone, V: Clone> IntoIterator for Cache<K, V> {
    type Item = (K, V);
    type IntoIter = std::collections::hash_map::IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        let now = Instant::now();
        self.items
            .into_iter()
            .filter(|(_, item)| item.expiration > now)
            .map(|(k, item)| (k, item.value))
            .collect::<HashMap<K, V>>()
            .into_iter()
    }
}

impl<K: Hash + Eq + Clone, V: Clone> Default for Cache<K, V> {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl<K: Hash + Eq + Clone, V: Clone> FromIterator<(K, V)>
    for Cache<K, V>
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut cache = Self::default();
        for (k, v) in iter {
            let _ = cache.insert(k, v);
        }
        cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_new_cache() {
        let cache: Cache<String, i32> =
            Cache::new(Duration::from_secs(60));
        assert!(cache.items.is_empty());
        assert_eq!(cache.ttl, Duration::from_secs(60));
    }

    #[test]
    fn test_insert_and_get() {
        let mut cache: Cache<String, i32> =
            Cache::new(Duration::from_secs(60));
        assert_eq!(cache.insert("key1".to_string(), 42), None);
        assert_eq!(cache.get(&"key1".to_string()), Some(&42));
    }

    #[test]
    fn test_insert_overwrite() {
        let mut cache: Cache<String, i32> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("key1".to_string(), 42);
        assert_eq!(cache.insert("key1".to_string(), 43), Some(42));
        assert_eq!(cache.get(&"key1".to_string()), Some(&43));
    }

    #[test]
    fn test_get_expired() {
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), 42);
        sleep(Duration::from_millis(150));
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_remove_expired() {
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), 42);
        let _ = cache.insert("key2".to_string(), 43);
        sleep(Duration::from_millis(150));
        cache.remove_expired();
        assert!(cache.items.is_empty());
    }

    #[test]
    fn test_contains_key() {
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), 42);
        assert!(cache.contains_key(&"key1".to_string()));
        assert!(!cache.contains_key(&"key2".to_string()));
        sleep(Duration::from_millis(150));
        assert!(!cache.contains_key(&"key1".to_string()));
    }

    #[test]
    fn test_ttl() {
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), 42);
        assert!(cache.ttl(&"key1".to_string()).is_some());
        assert!(
            cache.ttl(&"key1".to_string()).unwrap()
                <= Duration::from_millis(100)
        );
        assert_eq!(cache.ttl(&"key2".to_string()), None);
        sleep(Duration::from_millis(150));
        assert_eq!(cache.ttl(&"key1".to_string()), None);
    }

    #[test]
    fn test_refresh() {
        // Wide timing margin — shared CI runners (especially the macOS
        // matrix) can stall between sleeps for tens of ms. Use a 500 ms
        // TTL with 200 + 200 ms sleeps so the post-refresh checkpoint
        // sits at t=400 ms with the new expiry at t=200+500=700 ms.
        let mut cache = Cache::new(Duration::from_millis(500));
        let _ = cache.insert("key1".to_string(), 42);
        sleep(Duration::from_millis(200));
        assert!(cache.refresh(&"key1".to_string()));
        sleep(Duration::from_millis(200));
        assert_eq!(cache.get(&"key1".to_string()), Some(&42));
        assert!(!cache.refresh(&"key2".to_string()));
    }

    #[test]
    fn test_default() {
        let cache: Cache<String, i32> = Cache::default();
        assert!(cache.items.is_empty());
        assert_eq!(cache.ttl, Duration::from_secs(60));
    }

    #[test]
    fn test_multiple_types() {
        let mut cache: Cache<i32, String> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert(1, "one".to_string());
        assert_eq!(cache.get(&1), Some(&"one".to_string()));
    }

    #[test]
    fn test_large_cache() {
        let mut cache: Cache<String, i32> =
            Cache::new(Duration::from_secs(60));
        for i in 0..1000 {
            let _ = cache.insert(i.to_string(), i);
        }
        assert_eq!(cache.items.len(), 1000);
        for i in 0..1000 {
            assert_eq!(cache.get(&i.to_string()), Some(&i));
        }
    }

    #[test]
    fn test_set_capacity_evicts_lru_on_overflow() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        cache.set_capacity(2);

        let _ = cache.insert("key1".to_string(), "1".to_string());
        let _ = cache.insert("key2".to_string(), "2".to_string());
        // Third insert at capacity evicts the oldest entry (`key1`).
        let _ = cache.insert("key3".to_string(), "3".to_string());

        assert_eq!(
            cache.get(&"key1".to_string()),
            None,
            "oldest entry must be evicted under LRU"
        );
        assert_eq!(
            cache.get(&"key2".to_string()),
            Some(&"2".to_string())
        );
        assert_eq!(
            cache.get(&"key3".to_string()),
            Some(&"3".to_string())
        );
    }

    #[test]
    fn lru_promotes_on_get() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        cache.set_capacity(2);

        let _ = cache.insert("k1".to_string(), "1".to_string());
        let _ = cache.insert("k2".to_string(), "2".to_string());

        // Touch k1 — now k2 is the oldest.
        assert_eq!(
            cache.get(&"k1".to_string()),
            Some(&"1".to_string())
        );

        // Inserting k3 should evict k2, not k1.
        let _ = cache.insert("k3".to_string(), "3".to_string());
        assert_eq!(
            cache.get(&"k1".to_string()),
            Some(&"1".to_string())
        );
        assert_eq!(cache.get(&"k2".to_string()), None);
        assert_eq!(
            cache.get(&"k3".to_string()),
            Some(&"3".to_string())
        );
    }

    #[test]
    fn test_clear() {
        let mut cache: Cache<String, i32> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("key1".to_string(), 1);
        let _ = cache.insert("key2".to_string(), 2);

        assert_eq!(cache.get(&"key1".to_string()), Some(&1));
        assert_eq!(cache.get(&"key2".to_string()), Some(&2));

        cache.clear();

        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.get(&"key2".to_string()), None);
    }

    #[test]
    fn test_remove() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("key1".to_string(), "value1".to_string());

        assert_eq!(
            cache.remove(&"key1".to_string()),
            Some("value1".to_string())
        );
        assert_eq!(cache.remove(&"key1".to_string()), None);
    }

    #[test]
    fn test_update() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("key1".to_string(), "value1".to_string());

        assert!(cache.update(&"key1".to_string(), "value2".to_string()));
        assert_eq!(
            cache.get(&"key1".to_string()),
            Some(&"value2".to_string())
        );

        assert!(
            !cache.update(&"key2".to_string(), "value3".to_string())
        );
    }

    #[test]
    fn test_capacity() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        assert_eq!(cache.capacity, None);

        cache.set_capacity(100);
        assert_eq!(cache.capacity, Some(100));
    }

    #[test]
    fn test_iter() {
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), "value1".to_string());
        let _ = cache.insert("key2".to_string(), "value2".to_string());

        let items: Vec<(&String, &String)> = cache.iter().collect();
        assert_eq!(items.len(), 2);

        sleep(Duration::from_millis(150));

        let items: Vec<(&String, &String)> = cache.iter().collect();
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_from_iterator() {
        let items = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ];
        let mut cache: Cache<String, String> =
            items.into_iter().collect();
        assert_eq!(
            cache.get(&"key1".to_string()),
            Some(&"value1".to_string())
        );
        assert_eq!(
            cache.get(&"key2".to_string()),
            Some(&"value2".to_string())
        );
    }

    #[test]
    fn test_with_capacity() {
        let cache: Cache<String, String> =
            Cache::with_capacity(Duration::from_secs(60), 100);
        assert!(cache.items.capacity() >= 100);
    }

    #[test]
    fn into_iter_yields_live_entries_and_skips_expired() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("a".to_string(), "1".to_string());
        let _ = cache.insert("b".to_string(), "2".to_string());

        let mut items: Vec<_> = cache.into_iter().collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            items,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
            ]
        );
    }

    #[test]
    fn into_iter_drops_expired_entries() {
        let mut cache: Cache<String, i32> =
            Cache::new(Duration::from_millis(30));
        let _ = cache.insert("k".to_string(), 1);
        sleep(Duration::from_millis(60));
        let items: Vec<_> = cache.into_iter().collect();
        assert!(
            items.is_empty(),
            "expired entries must not be yielded via IntoIterator"
        );
    }

    // ── #40: CacheStats observability ───────────────────────────────

    #[test]
    fn stats_default_is_all_zeros() {
        let cache: Cache<String, u32> =
            Cache::new(Duration::from_secs(60));
        let s = cache.stats();
        assert_eq!(s.inserts, 0);
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 0);
        assert_eq!(s.evictions, 0);
        assert_eq!(s.ttl_expired, 0);
    }

    #[test]
    fn stats_inserts_count_includes_updates() {
        let mut cache: Cache<String, u32> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("k".to_string(), 1);
        let _ = cache.insert("k".to_string(), 2); // overwrite, still counts
        assert_eq!(cache.stats().inserts, 2);
    }

    #[test]
    fn stats_hit_and_miss_paths() {
        let mut cache: Cache<String, u32> =
            Cache::new(Duration::from_secs(60));
        let _ = cache.insert("a".to_string(), 1);
        let _ = cache.get(&"a".to_string()); // hit
        let _ = cache.get(&"a".to_string()); // hit
        let _ = cache.get(&"missing".to_string()); // miss
        let s = cache.stats();
        assert_eq!(s.hits, 2);
        assert_eq!(s.misses, 1);
    }

    #[test]
    fn stats_ttl_expired_path_through_get() {
        let mut cache: Cache<String, u32> =
            Cache::new(Duration::from_millis(20));
        let _ = cache.insert("k".to_string(), 1);
        sleep(Duration::from_millis(40));
        // get on an expired entry counts as both miss and ttl_expired.
        let _ = cache.get(&"k".to_string());
        let s = cache.stats();
        assert_eq!(s.ttl_expired, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.hits, 0);
    }

    #[test]
    fn stats_remove_expired_increments_ttl_counter() {
        let mut cache: Cache<String, u32> =
            Cache::new(Duration::from_millis(20));
        let _ = cache.insert("a".to_string(), 1);
        let _ = cache.insert("b".to_string(), 2);
        sleep(Duration::from_millis(40));
        cache.remove_expired();
        // Both entries reaped.
        assert_eq!(cache.stats().ttl_expired, 2);
    }

    #[test]
    fn stats_lru_eviction_counter_increments() {
        let mut cache: Cache<String, u32> =
            Cache::with_capacity(Duration::from_secs(60), 2);
        let _ = cache.insert("a".to_string(), 1);
        let _ = cache.insert("b".to_string(), 2);
        let _ = cache.insert("c".to_string(), 3); // evicts oldest (a)
        let _ = cache.insert("d".to_string(), 4); // evicts oldest (b)
        let s = cache.stats();
        assert_eq!(s.inserts, 4);
        assert_eq!(s.evictions, 2);
    }

    #[test]
    fn stats_struct_is_copy_and_default_and_eq() {
        // Compile-time checks via trait bounds.
        fn assert_copy_default_eq<T: Copy + Default + PartialEq>() {}
        assert_copy_default_eq::<CacheStats>();
        // Functional check: snapshots are independent of the cache.
        let mut cache: Cache<&str, u32> =
            Cache::new(Duration::from_secs(60));
        let s1 = cache.stats();
        let _ = cache.insert("k", 1);
        let s2 = cache.stats();
        assert_ne!(s1, s2); // mutating the cache doesn't mutate the snapshot
        assert_eq!(s1.inserts, 0);
        assert_eq!(s2.inserts, 1);
    }
}
