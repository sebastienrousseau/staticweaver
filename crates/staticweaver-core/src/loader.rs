// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Template sources and user extension points: the [`TemplateLoader`]
//! trait, the [`FilterFn`] / [`TestFn`] signatures, and the
//! [`Registry`] lookup the renderer uses to find them by name.

use crate::context::Value;
use crate::error::EngineError;
use alloc::{collections::BTreeMap, format, string::String, sync::Arc};

/// Signature of a user-registered filter, as accepted by
/// `staticweaver::Engine::add_filter`. The filter receives the
/// current pipeline value as `&str` and any colon-separated arguments
/// as `&[String]`, and returns the transformed value or an
/// `EngineError`. Wrapped in an `Arc` so an engine stays cheap to
/// clone.
///
/// # Examples
///
/// ```
/// use staticweaver_core::{EngineError, FilterFn};
/// use std::sync::Arc;
///
/// let slugify: FilterFn = Arc::new(
///     |input: &str, _args: &[String]| -> Result<String, EngineError> {
///         Ok(input
///             .chars()
///             .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
///             .collect())
///     },
/// );
/// assert_eq!(slugify("Hello World", &[]).unwrap(), "hello-world");
/// ```
pub type FilterFn = Arc<
    dyn Fn(&str, &[String]) -> Result<String, EngineError>
        + Send
        + Sync,
>;

/// Signature of a user-registered test, as accepted by
/// `staticweaver::Engine::add_test`. The test receives the operand
/// value the `is X` predicate is applied to and any extra arguments,
/// and returns a boolean (or an `EngineError` for malformed inputs).
/// Custom tests override built-ins (`defined`, `empty`, `none`) of
/// the same name.
///
/// # Examples
///
/// ```
/// use staticweaver_core::{TestFn, Value};
/// use std::sync::Arc;
///
/// let is_admin: TestFn = Arc::new(|v: &Value, _args: &[String]| {
///     Ok(matches!(v, Value::String(s) if s == "admin"))
/// });
/// assert!(is_admin(&Value::from("admin"), &[]).unwrap());
/// ```
pub type TestFn = Arc<
    dyn Fn(&Value, &[String]) -> Result<bool, EngineError>
        + Send
        + Sync,
>;

/// Source-of-truth for template files.
///
/// The facade crate's `FsLoader` reads templates from a directory on
/// disk, mapping `name` to `<root>/<name>.html`. Implementing this
/// trait yourself lets you serve templates from any backend: an
/// in-memory map (a `BTreeMap<String, String>` implements it), a
/// database, an embedded asset bundle, or a remote service.
///
/// Loaders are looked up whenever the renderer resolves a
/// `{{> partial}}`, a `{{#extends "base"}}`, or a named render. The
/// `name` passed in has already been validated against
/// path-traversal patterns (see [`crate::path::validate_name`]), so
/// implementations don't need to re-validate.
///
/// `Send + Sync` is required so an engine stays usable across
/// threads, e.g. behind an `Arc` in a web handler.
///
/// # Examples
///
/// ```
/// use staticweaver_core::TemplateLoader;
/// use std::collections::BTreeMap;
///
/// let mut loader = BTreeMap::new();
/// let _ = loader.insert("hello".to_string(), "Hi {{name}}!".to_string());
/// assert_eq!(loader.load("hello").unwrap(), "Hi {{name}}!");
/// assert!(loader.load("missing").is_err());
/// ```
pub trait TemplateLoader: Send + Sync {
    /// Loads the template named `name`. Implementations should
    /// return `EngineError::Io` (with the `std` feature) or
    /// `EngineError::ResourceNotFound` for missing or unreadable
    /// templates.
    ///
    /// # Errors
    ///
    /// Returns an error when no template named `name` exists or it
    /// cannot be read.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::TemplateLoader;
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// assert!(loader.load("missing").is_err());
    /// ```
    fn load(&self, name: &str) -> Result<String, EngineError>;
}

impl TemplateLoader for BTreeMap<String, String> {
    fn load(&self, name: &str) -> Result<String, EngineError> {
        self.get(name).cloned().ok_or_else(|| {
            EngineError::ResourceNotFound(format!(
                "no template named `{name}`"
            ))
        })
    }
}

/// Name-keyed lookup of user extensions ([`FilterFn`], [`TestFn`]).
///
/// The renderer only needs to find an entry by name, so any map can
/// back it. Implemented for `BTreeMap<String, T>` and, with the `std`
/// feature, for `std::collections::HashMap<String, T, S>`.
///
/// # Examples
///
/// ```
/// use staticweaver_core::Registry;
/// use std::collections::BTreeMap;
///
/// let mut m = BTreeMap::new();
/// let _ = m.insert("k".to_string(), 7);
/// assert_eq!(m.lookup("k"), Some(&7));
/// assert_eq!(m.lookup("missing"), None);
/// ```
pub trait Registry<T>: Send + Sync {
    /// Returns the entry registered under `name`, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::Registry;
    /// use std::collections::BTreeMap;
    ///
    /// let m: BTreeMap<String, u8> = BTreeMap::new();
    /// assert!(m.lookup("x").is_none());
    /// ```
    fn lookup(&self, name: &str) -> Option<&T>;
}

impl<T: Send + Sync> Registry<T> for BTreeMap<String, T> {
    fn lookup(&self, name: &str) -> Option<&T> {
        self.get(name)
    }
}

#[cfg(feature = "std")]
impl<T, S> Registry<T> for std::collections::HashMap<String, T, S>
where
    T: Send + Sync,
    S: core::hash::BuildHasher + Send + Sync,
{
    fn lookup(&self, name: &str) -> Option<&T> {
        self.get(name)
    }
}
