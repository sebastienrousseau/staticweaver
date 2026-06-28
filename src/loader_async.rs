// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Async loader (issue #37)
//!
//! Mirror of [`crate::engine::TemplateLoader`] for async runtimes.
//! Without this module, async users would have to spawn blocking
//! threads (`tokio::task::spawn_blocking`) to load templates — which
//! defeats the point of being in an async runtime in the first place.
//!
//! Gated behind the `async` feature so default builds stay
//! dep-light. The reference implementation `TokioFsLoader` is
//! further gated behind `async-tokio` so callers running on other
//! executors (async-std, smol, glommio) aren't forced to pull in
//! tokio. (Intentional plain code-span, not intradoc link —
//! `TokioFsLoader` is itself feature-gated and an intradoc link
//! would break the strict docs build under `--no-default-features`.)
//!
//! ## Bringing your own loader
//!
//! Implement `AsyncTemplateLoader` against whatever async backend
//! you have — a remote KV store, an embedded asset bundle hydrated
//! from disk, an HTTP CDN. The trait uses [`async fn` in traits
//! (AFIT)](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits)
//! so impls need MSRV ≥ 1.75 (this crate's MSRV).
//!
//! ```no_run
//! use staticweaver::loader_async::AsyncTemplateLoader;
//! use std::borrow::Cow;
//!
//! struct EchoLoader;
//!
//! impl AsyncTemplateLoader for EchoLoader {
//!     async fn load(&self, name: &str) -> Result<Cow<'_, str>, staticweaver::EngineError> {
//!         Ok(Cow::Owned(format!("loaded: {name}")))
//!     }
//! }
//! ```

use crate::error::EngineError;
use std::borrow::Cow;

/// Async counterpart to [`crate::engine::TemplateLoader`].
///
/// Implementations resolve a template name (the bare layout key passed
/// to `render_page_async` / `render_template_async`) to its source
/// text, asynchronously. Errors surface as [`EngineError`] so the
/// caller doesn't need to learn a second error vocabulary for async.
///
/// The returned [`Cow`] lets impls return either an owned `String`
/// (the common case — a network round-trip produces fresh bytes) or a
/// borrowed `&str` (when the loader holds the bytes already, e.g. an
/// in-memory bundle).
///
/// `Send + Sync` bound: an `Engine` carrying an `AsyncTemplateLoader`
/// must remain `Send + Sync` so callers can stash it in an
/// `Arc<Engine>` shared across executor tasks.
///
/// # Examples
///
/// ```no_run
/// // `ignore` because the surface needs the `async` feature; the
/// // code below is a faithful sketch of a custom impl.
/// use staticweaver::loader_async::AsyncTemplateLoader;
/// use staticweaver::EngineError;
/// use std::borrow::Cow;
///
/// struct EchoLoader;
///
/// impl AsyncTemplateLoader for EchoLoader {
///     async fn load(&self, name: &str) -> Result<Cow<'_, str>, EngineError> {
///         Ok(Cow::Owned(format!("loaded: {name}")))
///     }
/// }
/// ```
pub trait AsyncTemplateLoader: Send + Sync {
    /// Load the named template asynchronously.
    ///
    /// `load` is invoked by `Engine::render_template_async` /
    /// `render_page_async` rather than being called directly in normal use.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use staticweaver::loader_async::MemoryAsyncLoader;
    /// use staticweaver::{Context, Engine};
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// # async fn run() -> Result<(), staticweaver::EngineError> {
    /// let loader = MemoryAsyncLoader::new(HashMap::new());
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let ctx = Context::new();
    /// // Internally calls loader.load("page").await; surface is async.
    /// let _ = engine.render_template_async(&loader, "page", &ctx).await;
    /// # Ok(()) }
    /// ```
    fn load(
        &self,
        name: &str,
    ) -> impl std::future::Future<
        Output = Result<Cow<'_, str>, EngineError>,
    > + Send;
}

/// Default async loader backed by `tokio::fs`. Equivalent to the sync
/// [`crate::engine::FsLoader`] but uses non-blocking file reads.
///
/// Gated behind the `async-tokio` feature so callers using other
/// async runtimes don't pull in tokio just for filesystem reads.
///
/// # Examples
///
/// ```no_run
/// use staticweaver::loader_async::TokioFsLoader;
///
/// let loader = TokioFsLoader::new("templates");
/// // Pair with Engine::render_page_async(&loader, &ctx, "layout").await.
/// ```
#[cfg(feature = "async-tokio")]
#[derive(Debug, Clone)]
pub struct TokioFsLoader {
    root: std::path::PathBuf,
}

#[cfg(feature = "async-tokio")]
impl TokioFsLoader {
    /// Create a TokioFsLoader rooted at `root`. Templates are resolved
    /// relative to this directory — `loader.load("blog/post")` opens
    /// `<root>/blog/post`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use staticweaver::loader_async::TokioFsLoader;
    /// use std::path::PathBuf;
    ///
    /// let loader = TokioFsLoader::new(PathBuf::from("templates"));
    /// ```
    #[must_use]
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[cfg(feature = "async-tokio")]
impl AsyncTemplateLoader for TokioFsLoader {
    async fn load(
        &self,
        name: &str,
    ) -> Result<Cow<'_, str>, EngineError> {
        let path = self.root.join(name);
        let bytes =
            tokio::fs::read(&path).await.map_err(EngineError::Io)?;
        let text = String::from_utf8(bytes).map_err(|e| {
            EngineError::InvalidTemplate(format!(
                "template `{name}` is not valid UTF-8: {e}"
            ))
        })?;
        Ok(Cow::Owned(text))
    }
}

/// In-memory async loader. Useful for tests and for embedded use
/// where the template bytes ship inside the binary.
///
/// # Examples
///
/// ```no_run
/// use staticweaver::loader_async::MemoryAsyncLoader;
/// use std::collections::HashMap;
///
/// let mut store = HashMap::new();
/// let _ = store.insert("page".to_string(), "Hi {{name}}".to_string());
/// let loader = MemoryAsyncLoader::new(store);
/// // Pair with Engine::render_template_async(&loader, "page", &ctx).await.
/// ```
#[derive(Debug, Clone, Default)]
pub struct MemoryAsyncLoader {
    store: std::collections::HashMap<String, String>,
}

impl MemoryAsyncLoader {
    /// Build a `MemoryAsyncLoader` from a `(name -> body)` map.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use staticweaver::loader_async::MemoryAsyncLoader;
    /// use std::collections::HashMap;
    ///
    /// let store: HashMap<String, String> = HashMap::new();
    /// let loader = MemoryAsyncLoader::new(store);
    /// ```
    #[must_use]
    pub fn new(
        store: std::collections::HashMap<String, String>,
    ) -> Self {
        Self { store }
    }
}

impl AsyncTemplateLoader for MemoryAsyncLoader {
    async fn load(
        &self,
        name: &str,
    ) -> Result<Cow<'_, str>, EngineError> {
        self.store
            .get(name)
            .map(|s| Cow::Borrowed(s.as_str()))
            .ok_or_else(|| {
                EngineError::ResourceNotFound(format!(
                    "template `{name}` not found in MemoryAsyncLoader"
                ))
            })
    }
}
