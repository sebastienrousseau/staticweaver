// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Context Module
//!
//! Stores the key-value bindings consumed by the template engine. Values
//! are richer than plain strings: a `Value` enum carries `Null`, `Bool`,
//! `Number`, `String`, `List`, and `Map` variants, and the engine resolves
//! dot-separated lookups (`{{user.name}}`) by walking a `Value::Map`.

use fnv::FnvHashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A polymorphic context value.
///
/// Templates can substitute `Null`, `Bool`, `Number`, and `String` variants
/// directly via `Display`. `List` and `Map` are addressable through
/// dot-notation paths (e.g. `items.0` or `user.name`) and are typically
/// consumed by control-flow blocks rather than direct substitution.
///
/// # Examples
///
/// ```
/// use staticweaver::context::Value;
///
/// let v = Value::Number(42);
/// assert_eq!(v.to_string(), "42");
///
/// let nested = Value::Map({
///     let mut m = std::collections::HashMap::new();
///     let _ = m.insert("name".to_string(), Value::String("Alice".to_string()));
///     m.into_iter().collect()
/// });
/// assert!(matches!(nested.get_path("name"), Some(Value::String(_))));
/// ```
#[derive(Debug, Default, Clone, PartialEq)]
pub enum Value {
    /// Absent / null. Renders as the empty string.
    #[default]
    Null,
    /// Boolean. Renders as `"true"` or `"false"`.
    Bool(bool),
    /// 64-bit signed integer. Renders via `Display`.
    Number(i64),
    /// UTF-8 string. Renders verbatim (the engine handles HTML escaping).
    String(String),
    /// Ordered sequence of values. Addressable by `0`, `1`, ...
    List(Vec<Value>),
    /// Nested map of named values. Addressable by key.
    Map(FnvHashMap<String, Value>),
}

impl Value {
    /// Walks a dot-separated `path` through nested `Map` and `List`
    /// variants, returning the final value if the entire path resolves.
    ///
    /// `List` segments are matched as decimal indices (`items.0.name`).
    /// Returns `None` for any missing segment, type mismatch, or invalid
    /// index.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::context::Value;
    /// use fnv::FnvHashMap;
    ///
    /// let mut inner = FnvHashMap::default();
    /// let _ = inner.insert("name".to_string(), Value::String("Alice".to_string()));
    /// let mut outer = FnvHashMap::default();
    /// let _ = outer.insert("user".to_string(), Value::Map(inner));
    /// let v = Value::Map(outer);
    ///
    /// assert!(matches!(v.get_path("user.name"), Some(Value::String(_))));
    /// assert!(v.get_path("user.missing").is_none());
    /// ```
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut current = self;
        for segment in path.split('.') {
            current = match current {
                Value::Map(m) => m.get(segment)?,
                Value::List(items) => {
                    let idx: usize = segment.parse().ok()?;
                    items.get(idx)?
                }
                _ => return None,
            };
        }
        Some(current)
    }

    /// Returns `true` if the value is "truthy" — used by control-flow
    /// blocks (`{{#if key}}`).
    ///
    /// `Null` and `Bool(false)` are falsy; `Number(0)` is falsy; empty
    /// `String`, `List`, and `Map` are falsy. Everything else is truthy.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::context::Value;
    ///
    /// assert!(!Value::Null.is_truthy());
    /// assert!(!Value::Bool(false).is_truthy());
    /// assert!(!Value::Number(0).is_truthy());
    /// assert!(!Value::String(String::new()).is_truthy());
    /// assert!(Value::String("x".to_string()).is_truthy());
    /// assert!(Value::Number(1).is_truthy());
    /// ```
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0,
            Value::String(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Map(m) => !m.is_empty(),
        }
    }

    /// Returns the inner string for `Value::String`, otherwise `None`.
    /// Convenience for the back-compat `Context::get` shape.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::context::Value;
    ///
    /// assert_eq!(Value::String("hi".to_string()).as_str(), Some("hi"));
    /// assert_eq!(Value::Number(42).as_str(), None);
    /// ```
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    /// Renders a primitive `Value` as its substitution string. `List` and
    /// `Map` render as the empty string — the engine treats them as
    /// non-substitutable and reports an error if they appear in a `{{ }}`
    /// tag rather than a control-flow block.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => Ok(()),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::String(s) => f.write_str(s),
            Value::List(_) | Value::Map(_) => Ok(()),
        }
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Number(i64::from(n))
    }
}

impl<V: Into<Value>> From<Vec<V>> for Value {
    fn from(items: Vec<V>) -> Self {
        Value::List(items.into_iter().map(Into::into).collect())
    }
}

fn hash_value<H: Hasher>(v: &Value, h: &mut H) {
    // Tag each variant before hashing the payload so distinct types with
    // similar shapes (e.g. `Bool(true)` vs `Number(1)`) cannot collide.
    match v {
        Value::Null => 0u8.hash(h),
        Value::Bool(b) => {
            1u8.hash(h);
            b.hash(h);
        }
        Value::Number(n) => {
            2u8.hash(h);
            n.hash(h);
        }
        Value::String(s) => {
            3u8.hash(h);
            s.hash(h);
        }
        Value::List(items) => {
            4u8.hash(h);
            items.len().hash(h);
            for item in items {
                hash_value(item, h);
            }
        }
        Value::Map(m) => {
            5u8.hash(h);
            // Sort keys before feeding into the hasher: XOR aggregation
            // is order-independent but collision-prone (issue #30).
            // `FnvHashMap` iteration order is unspecified, so we must
            // sort to keep the hash deterministic.
            m.len().hash(h);
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_unstable();
            for k in keys {
                k.hash(h);
                hash_value(&m[k], h);
            }
        }
    }
}

/// Represents the context for template rendering.
///
/// Stores key → [`Value`] bindings. The shape is backwards compatible with
/// the previous `String → String` API: `set(key, value)` still accepts two
/// `String`s and stores them as a [`Value::String`]; `get(key)` still
/// returns `Option<&String>` for the `Value::String` case.
///
/// For nested data — needed by dot-notation paths like `{{user.name}}` —
/// use [`Context::set_value`] with a richer [`Value`] (typically built via
/// `Value::from` on a list, map, or primitive).
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
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Context {
    elements: FnvHashMap<String, Value>,
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
                fnv::FnvBuildHasher::default(),
            ),
        }
    }

    /// Stable, iteration-order-independent hash of every entry.
    ///
    /// Keys are sorted before being fed to the hasher (issue #30). The
    /// previous XOR aggregation was order-independent but collision-prone:
    /// two distinct `(key, value)` sets could XOR to the same `u64`,
    /// returning a stale render from the `render_page` cache.
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
    #[must_use]
    pub fn hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.elements.len().hash(&mut h);
        let mut keys: Vec<&String> = self.elements.keys().collect();
        keys.sort_unstable();
        for k in keys {
            k.hash(&mut h);
            hash_value(&self.elements[k], &mut h);
        }
        h.finish()
    }

    /// Sets a string-typed entry. Backwards-compatible with the pre-Value
    /// API; internally wraps the value as [`Value::String`].
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
        let _ = self.elements.insert(key, Value::String(value));
    }

    /// Stores any `Into<Value>` payload. Use this for non-string contexts
    /// (booleans, numbers, lists, nested maps).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, context::Value};
    ///
    /// let mut ctx = Context::new();
    /// ctx.set_value("count".to_string(), 42);
    /// ctx.set_value("active".to_string(), true);
    /// ctx.set_value("tags".to_string(), vec!["rust", "ssg"]);
    ///
    /// assert!(matches!(ctx.get_value("count"), Some(Value::Number(42))));
    /// assert!(matches!(ctx.get_value("active"), Some(Value::Bool(true))));
    /// ```
    pub fn set_value<V: Into<Value>>(&mut self, key: String, value: V) {
        let _ = self.elements.insert(key, value.into());
    }

    /// Borrowed-key counterpart to [`Context::set_value`]. Allocates a
    /// new `String` for the key only when no entry with that key
    /// exists; on update the existing key is reused and only the value
    /// slot is overwritten. Use this in tight loops where the same key
    /// is set repeatedly — the `#each` iterator helpers (`this`,
    /// `@index`, `@first`, `@last`, `@key`) are the canonical case.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut ctx = Context::new();
    /// // First call allocates the key; second call only overwrites the value.
    /// ctx.set_value_str("count", 1);
    /// ctx.set_value_str("count", 2);
    /// assert_eq!(format!("{:?}", ctx.get_value("count")), "Some(Number(2))");
    /// ```
    /// Borrowed-string variant of [`Context::set_value_str`] for
    /// `Value::String` entries. When the existing slot is already a
    /// `Value::String`, this clears its buffer and pushes the new bytes
    /// in place, *reusing the heap allocation* instead of building a
    /// fresh `String`. For tight loops that set the same string-keyed
    /// slot repeatedly (most notably `#each` binding `this` to a
    /// `Value::String` item on every iteration) this saves one heap
    /// allocation per call after the first. Type changes (slot was a
    /// `Number`/`Bool`/etc) fall through to a full overwrite.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut ctx = Context::new();
    /// ctx.set_value_string("greeting", "Hello");
    /// ctx.set_value_string("greeting", "Hi"); // reuses the buffer
    /// ```
    pub fn set_value_string(&mut self, key: &str, value: &str) {
        match self.elements.get_mut(key) {
            Some(Value::String(dst)) => {
                dst.clear();
                dst.push_str(value);
            }
            Some(slot) => *slot = Value::String(value.to_string()),
            None => {
                let _ = self.elements.insert(
                    key.to_string(),
                    Value::String(value.to_string()),
                );
            }
        }
    }

    /// Borrowed-key counterpart to [`Context::set_value`]. Allocates a
    /// new `String` for the key only when no entry with that key
    /// exists; on update the existing key is reused and only the value
    /// slot is overwritten. Use this in tight loops where the same key
    /// is set repeatedly — the `#each` iterator helpers (`this`,
    /// `@index`, `@first`, `@last`, `@key`) are the canonical case.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut ctx = Context::new();
    /// // First call allocates the key; second only overwrites the value.
    /// ctx.set_value_str("count", 1);
    /// ctx.set_value_str("count", 2);
    /// assert_eq!(format!("{:?}", ctx.get_value("count")), "Some(Number(2))");
    /// ```
    pub fn set_value_str<V: Into<Value>>(
        &mut self,
        key: &str,
        value: V,
    ) {
        if let Some(slot) = self.elements.get_mut(key) {
            *slot = value.into();
        } else {
            let _ = self.elements.insert(key.to_string(), value.into());
        }
    }

    /// Returns the inner string for a `Value::String` entry, matching the
    /// historical signature. Non-string values return `None` (use
    /// [`Context::get_value`] or [`Context::get_path`] instead).
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
        match self.elements.get(key)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the raw `Value` for a top-level key.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, context::Value};
    ///
    /// let mut ctx = Context::new();
    /// ctx.set_value("count".to_string(), 7);
    /// assert!(matches!(ctx.get_value("count"), Some(Value::Number(7))));
    /// ```
    #[must_use]
    pub fn get_value(&self, key: &str) -> Option<&Value> {
        self.elements.get(key)
    }

    /// Resolves a dot-separated path through nested `Map` and `List`
    /// values. Returns `None` for any missing segment or type mismatch.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, context::Value};
    /// use fnv::FnvHashMap;
    ///
    /// let mut user = FnvHashMap::default();
    /// let _ = user.insert("name".to_string(), Value::String("Ada".to_string()));
    ///
    /// let mut ctx = Context::new();
    /// ctx.set_value("user".to_string(), Value::Map(user));
    ///
    /// assert!(matches!(ctx.get_path("user.name"), Some(Value::String(_))));
    /// assert!(ctx.get_path("user.missing").is_none());
    /// ```
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut parts = path.splitn(2, '.');
        let head = parts.next()?;
        let value = self.elements.get(head)?;
        match parts.next() {
            Some(rest) => value.get_path(rest),
            None => Some(value),
        }
    }

    /// Returns a mutable reference to the inner string for a
    /// `Value::String` entry. Returns `None` for any other variant.
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
        match self.elements.get_mut(key)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Removes a key. Returns the inner string when the prior value was a
    /// `Value::String`; returns `None` for any other type or missing key.
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
        match self.elements.remove(key)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the number of top-level entries.
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

    /// Capacity of the underlying map.
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

    /// `true` when the context has no entries.
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

    /// Iterator over `(key, &Value)` pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("k".to_string(), "v".to_string());
    /// for (key, _value) in context.iter() {
    ///     assert_eq!(key, "k");
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.elements.iter()
    }

    /// Removes every entry.
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

    /// Update or insert a string-valued entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Context;
    ///
    /// let mut context = Context::new();
    /// context.set("key".to_string(), "old_value".to_string());
    /// context.update("key", "new_value");
    /// assert_eq!(context.get("key"), Some(&"new_value".to_string()));
    /// ```
    pub fn update<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        let _ = self
            .elements
            .insert(key.into(), Value::String(value.into()));
    }
}

impl FromIterator<(String, String)> for Context {
    /// Builds a context from `(String, String)` pairs (back-compat shape).
    /// Each value is wrapped as `Value::String`.
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
    /// Extends the context from `(String, String)` pairs (back-compat
    /// shape).
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
        let pairs: Vec<_> = context.iter().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "key1");
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

    #[test]
    fn test_update() {
        let mut context = Context::new();
        context.set("key".to_string(), "value".to_string());
        context.update("key", "new_value");
        assert_eq!(context.get("key"), Some(&"new_value".to_string()));
    }

    // ── Value-level tests ───────────────────────────────────────────

    #[test]
    fn set_value_stores_typed_payload() {
        let mut ctx = Context::new();
        ctx.set_value("count".to_string(), 42);
        ctx.set_value("active".to_string(), true);
        ctx.set_value("nothing".to_string(), Value::Null);

        assert!(matches!(
            ctx.get_value("count"),
            Some(Value::Number(42))
        ));
        assert!(matches!(
            ctx.get_value("active"),
            Some(Value::Bool(true))
        ));
        assert!(matches!(ctx.get_value("nothing"), Some(Value::Null)));
    }

    #[test]
    fn get_returns_none_for_non_string_values() {
        let mut ctx = Context::new();
        ctx.set_value("count".to_string(), 7);
        assert_eq!(ctx.get("count"), None);
        assert!(ctx.get_value("count").is_some());
    }

    #[test]
    fn get_path_walks_nested_maps() {
        let mut user = FnvHashMap::default();
        let _ = user.insert("name".to_string(), Value::from("Ada"));
        let _ = user.insert("age".to_string(), Value::Number(36));

        let mut ctx = Context::new();
        ctx.set_value("user".to_string(), Value::Map(user));

        assert_eq!(
            ctx.get_path("user.name").and_then(Value::as_str),
            Some("Ada"),
        );
        assert!(matches!(
            ctx.get_path("user.age"),
            Some(Value::Number(36))
        ));
        assert!(ctx.get_path("user.missing").is_none());
        assert!(ctx.get_path("missing.name").is_none());
    }

    #[test]
    fn get_path_walks_lists_by_numeric_index() {
        let mut ctx = Context::new();
        ctx.set_value(
            "items".to_string(),
            vec!["alpha", "beta", "gamma"],
        );
        assert_eq!(
            ctx.get_path("items.0").and_then(Value::as_str),
            Some("alpha"),
        );
        assert_eq!(
            ctx.get_path("items.2").and_then(Value::as_str),
            Some("gamma"),
        );
        assert!(ctx.get_path("items.99").is_none());
        assert!(ctx.get_path("items.notanumber").is_none());
    }

    #[test]
    fn value_display_renders_primitives_only() {
        assert_eq!(Value::Null.to_string(), "");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Number(42).to_string(), "42");
        assert_eq!(Value::String("hi".to_string()).to_string(), "hi");
        // List + Map render empty so that {{key}} on a non-leaf returns
        // an empty substitution rather than something nonsensical.
        let lst: Value = vec!["x"].into();
        assert_eq!(lst.to_string(), "");
    }

    #[test]
    fn value_truthiness_matches_documented_table() {
        assert!(!Value::Null.is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Number(0).is_truthy());
        assert!(Value::Number(-1).is_truthy());
        assert!(!Value::String(String::new()).is_truthy());
        assert!(Value::String("x".to_string()).is_truthy());
        assert!(!Value::List(vec![]).is_truthy());
        assert!(Value::List(vec![Value::Null]).is_truthy());
    }

    #[test]
    fn value_from_string_owned() {
        let s = "owned".to_string();
        let v = Value::from(s);
        assert_eq!(v.as_str(), Some("owned"));
    }

    #[test]
    fn value_map_truthiness() {
        let mut m = FnvHashMap::default();
        assert!(!Value::Map(m.clone()).is_truthy());
        let _ = m.insert("k".to_string(), Value::Null);
        assert!(Value::Map(m).is_truthy());
    }

    #[test]
    fn hash_distinguishes_value_variants_with_same_payload() {
        // Bool(true) and Number(1) must not collide under the
        // tagged-variant hashing scheme.
        let mut a = Context::new();
        a.set_value("k".to_string(), true);
        let mut b = Context::new();
        b.set_value("k".to_string(), 1i64);
        assert_ne!(a.hash(), b.hash());

        // Null and String("") must not collide.
        let mut c = Context::new();
        c.set_value("k".to_string(), Value::Null);
        let mut d = Context::new();
        d.set_value("k".to_string(), "");
        assert_ne!(c.hash(), d.hash());
    }

    #[test]
    fn hash_value_list_and_map() {
        let mut a = Context::new();
        a.set_value("k".to_string(), vec![1, 2]);
        let mut b = Context::new();
        b.set_value("k".to_string(), vec![1, 2]);
        assert_eq!(a.hash(), b.hash());

        let mut c = Context::new();
        let mut m = FnvHashMap::default();
        let _ = m.insert("x".to_string(), Value::Number(1));
        c.set_value("m".to_string(), Value::Map(m));

        let mut d = Context::new();
        let mut m2 = FnvHashMap::default();
        let _ = m2.insert("x".to_string(), Value::Number(1));
        d.set_value("m".to_string(), Value::Map(m2));
        assert_eq!(c.hash(), d.hash());
    }

    #[test]
    fn context_get_non_string_returns_none() {
        let mut ctx = Context::new();
        ctx.set_value("k".to_string(), 42);
        assert_eq!(ctx.get("k"), None);
    }

    #[test]
    fn context_get_mut_non_string_returns_none() {
        let mut ctx = Context::new();
        ctx.set_value("k".to_string(), 42);
        assert_eq!(ctx.get_mut("k"), None);
    }

    #[test]
    fn context_remove_non_string_returns_none() {
        let mut ctx = Context::new();
        ctx.set_value("k".to_string(), 42);
        assert_eq!(ctx.remove("k"), None);
        assert!(ctx.is_empty());
    }

    #[test]
    fn value_as_str_non_string_returns_none() {
        assert_eq!(Value::Null.as_str(), None);
        assert_eq!(Value::Bool(true).as_str(), None);
        assert_eq!(Value::Number(1).as_str(), None);
    }

    #[test]
    fn value_get_path_mismatch_returns_none() {
        let v = Value::Number(42);
        assert_eq!(v.get_path("any.path"), None);

        let list = Value::from(vec![1]);
        assert_eq!(list.get_path("0.child"), None);
    }

    #[test]
    fn value_from_i32_and_i64() {
        let v32 = Value::from(42i32);
        let v64 = Value::from(42i64);
        assert_eq!(v32, v64);
    }

    #[test]
    fn context_get_path_empty_returns_none() {
        let ctx = Context::new();
        // splitn(2, '.') on empty string yields [""]
        // elements.get("") returns None.
        assert_eq!(ctx.get_path(""), None);
    }

    // ── #30: hash collision-resistance ──────────────────────────────

    #[test]
    fn hash_distinguishes_swapped_values() {
        // The classic XOR-aggregation failure mode: swap two values
        // between two keys. Sort-then-hash distinguishes the resulting
        // contexts; XOR aggregation may not.
        let mut a = Context::new();
        a.set("k1".to_string(), "v1".to_string());
        a.set("k2".to_string(), "v2".to_string());

        let mut b = Context::new();
        b.set("k1".to_string(), "v2".to_string());
        b.set("k2".to_string(), "v1".to_string());

        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_insertion_order_independent() {
        // Equal logical sets must hash equal regardless of insertion
        // order — the `render_page` cache relies on this invariant.
        let mut a = Context::new();
        a.set("alpha".to_string(), "1".to_string());
        a.set("beta".to_string(), "2".to_string());
        a.set("gamma".to_string(), "3".to_string());

        let mut b = Context::new();
        b.set("gamma".to_string(), "3".to_string());
        b.set("alpha".to_string(), "1".to_string());
        b.set("beta".to_string(), "2".to_string());

        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn hash_distinguishes_empty_from_single_entry() {
        let empty = Context::new();
        let mut one = Context::new();
        one.set("k".to_string(), "v".to_string());
        assert_ne!(empty.hash(), one.hash());
    }

    #[test]
    fn hash_nested_map_is_order_independent() {
        // Value::Map iteration order is unspecified (FnvHashMap).
        // Sort-then-hash inside hash_value() keeps the digest stable
        // for equal maps inserted in different orders.
        let mut m1 = FnvHashMap::default();
        let _ = m1.insert("a".to_string(), Value::Number(1));
        let _ = m1.insert("b".to_string(), Value::Number(2));
        let _ = m1.insert("c".to_string(), Value::Number(3));

        let mut m2 = FnvHashMap::default();
        let _ = m2.insert("c".to_string(), Value::Number(3));
        let _ = m2.insert("a".to_string(), Value::Number(1));
        let _ = m2.insert("b".to_string(), Value::Number(2));

        let mut a = Context::new();
        a.set_value("m".to_string(), Value::Map(m1));
        let mut b = Context::new();
        b.set_value("m".to_string(), Value::Map(m2));

        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn hash_nested_map_distinguishes_swapped_values() {
        // Same swap-pattern test as `hash_distinguishes_swapped_values`
        // but inside a `Value::Map`, exercising the inner hash_value
        // path.
        let mut m1 = FnvHashMap::default();
        let _ = m1.insert("k1".to_string(), Value::from("v1"));
        let _ = m1.insert("k2".to_string(), Value::from("v2"));

        let mut m2 = FnvHashMap::default();
        let _ = m2.insert("k1".to_string(), Value::from("v2"));
        let _ = m2.insert("k2".to_string(), Value::from("v1"));

        let mut a = Context::new();
        a.set_value("m".to_string(), Value::Map(m1));
        let mut b = Context::new();
        b.set_value("m".to_string(), Value::Map(m2));

        assert_ne!(a.hash(), b.hash());
    }
}
