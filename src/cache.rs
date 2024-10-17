// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// Represents a cached item with its value and expiration time.
#[derive(Debug, Clone)]
struct CachedItem<T> {
    value: T,
    expiration: Instant,
}

/// A simple cache implementation with expiration and optional capacity limit.
///
/// This cache provides time-based expiration for items and an optional maximum capacity.
/// It's designed to be generic over both key and value types for maximum flexibility.
#[derive(Debug, Clone)]
pub struct Cache<K, V> {
    items: HashMap<K, CachedItem<V>>,
    ttl: Duration,
    capacity: Option<usize>,
}

impl<K: Hash + Eq, V: Clone> Cache<K, V> {
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
        }
    }

    /// Returns an iterator over the key-value pairs in the cache.
    ///
    /// # Returns
    ///
    /// An iterator over the key-value pairs in the cache.
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
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache is at capacity and the key doesn't already exist, the new item won't be inserted.
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
        if let Some(cap) = self.capacity {
            if self.items.len() >= cap && !self.items.contains_key(&key)
            {
                return None; // Cache is at capacity
            }
        }
        let expiration = Instant::now() + self.ttl;
        self.items
            .insert(key, CachedItem { value, expiration })
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
    pub fn get(&self, key: &K) -> Option<&V> {
        self.items.get(key).and_then(|item| {
            if item.expiration > Instant::now() {
                Some(&item.value)
            } else {
                None
            }
        })
    }

    /// Removes expired items from the cache.
    ///
    /// This method should be called periodically to clean up the cache.
    ///
    /// Time complexity: O(n) where n is the number of items in the cache.
    pub fn remove_expired(&mut self) {
        let now = Instant::now();
        self.items.retain(|_, item| item.expiration > now);
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
    pub fn refresh(&mut self, key: &K) -> bool {
        if let Some(item) = self.items.get_mut(key) {
            item.expiration = Instant::now() + self.ttl;
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
    pub fn update(&mut self, key: &K, value: V) -> bool {
        if let Some(item) = self.items.get_mut(key) {
            item.value = value;
            item.expiration = Instant::now() + self.ttl;
            true
        } else {
            false
        }
    }

    /// Sets a maximum capacity for the cache.
    /// If the cache is already larger than this capacity, it will not remove items,
    /// but it will prevent new items from being added until it's below capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The maximum number of items the cache can hold.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = Some(capacity);
    }

    /// Clears all items from the cache.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Returns the number of items in the cache.
    ///
    /// # Returns
    ///
    /// The number of key-value pairs currently in the cache.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Checks if the cache is empty.
    ///
    /// # Returns
    ///
    /// `true` if the cache is empty, `false` otherwise.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<K: Hash + Eq, V: Clone> IntoIterator for Cache<K, V> {
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

impl<K: Hash + Eq, V: Clone> Default for Cache<K, V> {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl<K: Hash + Eq, V: Clone> FromIterator<(K, V)> for Cache<K, V> {
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
        let mut cache = Cache::new(Duration::from_millis(100));
        let _ = cache.insert("key1".to_string(), 42);
        sleep(Duration::from_millis(50));
        assert!(cache.refresh(&"key1".to_string()));
        sleep(Duration::from_millis(75));
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
    fn test_set_capacity() {
        let mut cache: Cache<String, String> =
            Cache::new(Duration::from_secs(60));
        cache.set_capacity(2);

        assert_eq!(
            cache.insert("key1".to_string(), "1".to_string()),
            None
        );
        assert_eq!(
            cache.insert("key2".to_string(), "2".to_string()),
            None
        );
        assert_eq!(
            cache.insert("key3".to_string(), "3".to_string()),
            None
        );

        assert_eq!(
            cache.get(&"key1".to_string()),
            Some(&"1".to_string())
        );
        assert_eq!(
            cache.get(&"key2".to_string()),
            Some(&"2".to_string())
        );
        assert_eq!(cache.get(&"key3".to_string()), None);
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
        cache.insert("key1".to_string(), "value1".to_string());

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
        cache.insert("key1".to_string(), "value1".to_string());

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
        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());

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
        let cache: Cache<String, String> = items.into_iter().collect();
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
}
