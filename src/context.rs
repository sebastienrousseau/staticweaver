// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

// src/context.rs

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Represents the context for template rendering.
///
/// `Context` holds key-value pairs that can be used to populate
/// placeholders in a template during the rendering process.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Context {
    /// The internal storage for context key-value pairs.
    pub elements: HashMap<String, String>,
}

impl Context {
    /// Computes a hash of the context.
    ///
    /// This method is used for caching purposes.
    ///
    /// # Returns
    ///
    /// A `u64` representing the hash of the context.
    pub(crate) fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (key, value) in &self.elements {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Creates a new, empty `Context`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let context = Context::new();
    /// assert!(context.elements.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Context {
        Context {
            elements: HashMap::new(),
        }
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
    /// context.set("name".to_string(), "Alice".to_string()); // Corrected to use String
    /// assert_eq!(context.get("name"), Some(&"Alice".to_string()));
    /// ```
    ///
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
    fn from_iter<I: IntoIterator<Item = (String, String)>>(
        iter: I,
    ) -> Self {
        let mut context = Context::new();
        for (key, value) in iter {
            context.set(key, value);
        }
        context
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
    fn test_set_and_get() {
        let mut context = Context::new();
        context.set("key".to_string(), "value".to_string());
        assert_eq!(context.get("key"), Some(&"value".to_string()));
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
