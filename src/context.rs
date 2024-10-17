// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Context Module
//!
//! This module provides the `Context` struct, which is used to store and manage
//! key-value pairs for template rendering. It offers a flexible and efficient way
//! to handle template variables and their values.
//!
//! The `Context` struct uses `FnvHashMap` for efficient string-based key lookups
//! and provides methods for manipulating and querying the stored data.

use fnv::FnvHashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Represents the context for template rendering.
///
/// `Context` holds key-value pairs that can be used to populate
/// placeholders in a template during the rendering process. It uses
/// `FnvHashMap` for efficient string-based key lookups.
///
/// # Examples
///
/// ```
/// use staticweaver::Context;
///
/// let mut context = Context::default();
/// context.set("name".to_string(), "Alice".to_string());
/// assert_eq!(context.get("name"), Some(&"Alice".to_string()));
/// ```
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Context {
    /// The internal storage for context key-value pairs.
    pub elements: FnvHashMap<String, String>,
}

impl Context {
    /// Creates a new, empty `Context`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let context = Context::new();
    /// assert!(context.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `Context` with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let context = Context::with_capacity(10);
    /// assert!(context.is_empty());
    /// assert!(context.capacity() >= 10);
    /// ```
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            elements: FnvHashMap::with_capacity_and_hasher(
                capacity,
                Default::default(),
            ),
        }
    }

    /// Computes a hash of the context.
    ///
    /// This method is used for caching purposes. It creates a stable hash
    /// based on the current state of the context.
    ///
    /// # Returns
    ///
    /// A `u64` representing the hash of the context.
    ///
    /// # Panics
    ///
    /// This method will panic if the hash computation overflows, which is
    /// extremely unlikely in practical scenarios.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("key".to_string(), "value".to_string());
    /// let hash = context.hash();
    /// assert_ne!(hash, 0);
    /// ```
    pub fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (key, value) in &self.elements {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Sets a key-value pair in the context.
    ///
    /// If the key already exists, its value will be updated.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to set.
    /// * `value` - The value to associate with the key.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("name".to_string(), "Alice".to_string());
    /// assert_eq!(context.get("name"), Some(&"Alice".to_string()));
    /// ```
    pub fn set(&mut self, key: String, value: String) {
        let _ = self.elements.insert(key, value);
    }

    /// Retrieves the value associated with a key from the context.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the value if the key exists,
    /// or `None` if it doesn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("name".to_string(), "Bob".to_string());
    /// assert_eq!(context.get("name"), Some(&"Bob".to_string()));
    /// assert_eq!(context.get("age"), None);
    /// ```
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&String> {
        self.elements.get(key)
    }

    /// Retrieves a mutable reference to the value associated with a key from the context.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up.
    ///
    /// # Returns
    ///
    /// An `Option` containing a mutable reference to the value if the key exists,
    /// or `None` if it doesn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("name".to_string(), "Bob".to_string());
    /// if let Some(value) = context.get_mut("name") {
    ///     *value = "Alice".to_string();
    /// }
    /// assert_eq!(context.get("name"), Some(&"Alice".to_string()));
    /// ```
    pub fn get_mut(&mut self, key: &str) -> Option<&mut String> {
        self.elements.get_mut(key)
    }

    /// Removes a key-value pair from the context.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to remove.
    ///
    /// # Returns
    ///
    /// The value associated with the key, or `None` if the key didn't exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("name".to_string(), "Alice".to_string());
    /// assert_eq!(context.remove("name"), Some("Alice".to_string()));
    /// assert_eq!(context.get("name"), None);
    /// ```
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.elements.remove(key)
    }

    /// Returns the number of elements in the context.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("key".to_string(), "value".to_string());
    /// assert_eq!(context.len(), 1);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Returns the number of elements the context can hold without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let context = Context::with_capacity(10);
    /// assert!(context.capacity() >= 10);
    /// ```
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.elements.capacity()
    }

    /// Returns true if the context contains no elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let context = Context::new();
    /// assert!(context.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Returns an iterator over the context's key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("key1".to_string(), "value1".to_string());
    /// context.set("key2".to_string(), "value2".to_string());
    ///
    /// for (key, value) in context.iter() {
    ///     println!("{}: {}", key, value);
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.elements.iter()
    }

    /// Removes all key-value pairs from the context.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("key".to_string(), "value".to_string());
    /// assert!(!context.is_empty());
    ///
    /// context.clear();
    /// assert!(context.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.elements.clear();
    }
}

impl FromIterator<(String, String)> for Context {
    /// Creates a `Context` from an iterator of key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let pairs = vec![
    ///     ("key1".to_string(), "value1".to_string()),
    ///     ("key2".to_string(), "value2".to_string()),
    /// ];
    /// let context: Context = pairs.into_iter().collect();
    /// assert_eq!(context.get("key1"), Some(&"value1".to_string()));
    /// assert_eq!(context.get("key2"), Some(&"value2".to_string()));
    /// ```
    fn from_iter<I: IntoIterator<Item = (String, String)>>(
        iter: I,
    ) -> Self {
        let mut context = Context::new();
        context.extend(iter);
        context
    }
}

impl Extend<(String, String)> for Context {
    /// Extends the context with the contents of the specified iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.extend(vec![
    ///     ("key1".to_string(), "value1".to_string()),
    ///     ("key2".to_string(), "value2".to_string()),
    /// ]);
    /// assert_eq!(context.get("key1"), Some(&"value1".to_string()));
    /// assert_eq!(context.get("key2"), Some(&"value2".to_string()));
    /// ```
    fn extend<T: IntoIterator<Item = (String, String)>>(
        &mut self,
        iter: T,
    ) {
        for (key, value) in iter {
            self.set(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_context() {
        let context = Context::new();
        assert!(context.is_empty());
    }

    #[test]
    fn test_with_capacity() {
        let context = Context::with_capacity(10);
        assert!(context.capacity() >= 10);
    }

    #[test]
    fn test_set_and_get() {
        let mut context = Context::new();
        context.set("key".to_string(), "value".to_string());
        assert_eq!(context.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_get_mut() {
        let mut context = Context::new();
        context.set("key".to_string(), "value".to_string());
        if let Some(value) = context.get_mut("key") {
            *value = "new_value".to_string();
        }
        assert_eq!(context.get("key"), Some(&"new_value".to_string()));
    }

    #[test]
    fn test_remove() {
        let mut context = Context::new();
        context.set("key".to_string(), "value".to_string());
        assert_eq!(context.remove("key"), Some("value".to_string()));
        assert_eq!(context.get("key"), None);
    }

    #[test]
    fn test_hash() {
        let mut context1 = Context::new();
        context1.set("key1".to_string(), "value1".to_string());

        let mut context2 = Context::new();
        context2.set("key1".to_string(), "value1".to_string());

        assert_eq!(context1.hash(), context2.hash());

        context2.set("key2".to_string(), "value2".to_string());
        assert_ne!(context1.hash(), context2.hash());
    }

    #[test]
    fn test_from_iterator() {
        let pairs = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ];
        let context: Context = pairs.into_iter().collect();
        assert_eq!(context.get("key1"), Some(&"value1".to_string()));
        assert_eq!(context.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_extend() {
        let mut context = Context::new();
        context.extend(vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ]);
        assert_eq!(context.get("key1"), Some(&"value1".to_string()));
        assert_eq!(context.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_iter() {
        let mut context = Context::new();
        context.set("key1".to_string(), "value1".to_string());
        context.set("key2".to_string(), "value2".to_string());

        let mut pairs: Vec<(&String, &String)> =
            context.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));

        assert_eq!(
            pairs,
            vec![
                (&"key1".to_string(), &"value1".to_string()),
                (&"key2".to_string(), &"value2".to_string()),
            ]
        );
    }

    #[test]
    fn test_clear() {
        let mut context = Context::new();
        context.set("key1".to_string(), "value1".to_string());
        context.set("key2".to_string(), "value2".to_string());
        assert_eq!(context.len(), 2);

        context.clear();
        assert!(context.is_empty());
    }
}
