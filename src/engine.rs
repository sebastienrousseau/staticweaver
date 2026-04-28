// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Engine Module
//!
//! The template rendering engine. The [`Engine`] struct holds the
//! template root directory, the render cache, and the active delimiter
//! pair, and exposes [`Engine::render_template`] (string-in, string-out)
//! and [`Engine::render_page`] (read a `.html` layout from disk).
//!
//! ## Templating language
//!
//! - **Substitution**: `{{key}}` looks up a `Context` value and emits
//!   it. Default behaviour HTML-escapes `& < > " '`. Use `{{!key}}` to
//!   emit the value verbatim, or
//!   [`Engine::with_html_escape(false)`](Engine::with_html_escape) to
//!   disable escape globally.
//! - **Dot-notation**: `{{user.email}}` walks a [`Value::Map`](crate::context::Value);
//!   `{{items.0}}` indexes a [`Value::List`](crate::context::Value).
//! - **Control flow**: `{{#if EXPR}}…{{else}}…{{/if}}` and
//!   `{{#each list}}…{{/each}}`. Each-blocks expose the loop helpers
//!   `@index`, `@first`, `@last`, and (for `Map` iteration) `@key`,
//!   binding each element to `this`.
//! - **Expressions** (inside `#if`): comparisons (`==`, `!=`, `<`,
//!   `<=`, `>`, `>=`), short-circuiting boolean operators (`and`,
//!   `or`, `not`), checked integer math (`+`, `-`, `*`, `/`), and
//!   postfix tests (`is defined`, `is empty`, `is none`) with
//!   `is not` for negation. Precedence: postfix tests bind tightest,
//!   then math, comparisons, `not`, `and`, `or`. Bare paths like
//!   `{{#if user}}` keep their truthiness semantics.
//! - **Partials**: `{{> name}}` reads `name.html` from
//!   `template_path` and substitutes the parent context. Pass scoped
//!   parameters via `{{> name k=v}}`. Recursion is capped at depth 10.
//! - **Inheritance**: `{{#extends "base"}}` plus
//!   `{{#block "name"}}…{{/block}}` lets a child template override
//!   named blocks in its parent. Multi-level chains compose; the
//!   child wins on conflicting block names.
//! - **In-template assignment**: `{{#set name = LITERAL}}` binds a
//!   value locally for subsequent tags. Local-scope only — does not
//!   leak into the parent context.
//! - **Filters**: `{{ x | filter }}`, with arguments via
//!   `{{ x | filter:arg }}`. Built-in filters: `uppercase`,
//!   `lowercase`, `trim`, `truncate`, `capitalize`, `length`,
//!   `default`, `replace`, `urlencode`, `safe`.
//! - **Comments**: `{{! one-line }}` and `{{!-- multi-line --}}` are
//!   stripped before rendering.
//! - **Whitespace control**: `{{- key -}}` trims adjacent whitespace
//!   on the corresponding side of the tag.
//! - **Backslash escape**: `\{{literal}}` emits `{{literal}}`
//!   verbatim. Even runs collapse to literal backslashes, odd runs
//!   escape the following delimiter.
//! - **Custom delimiters**:
//!   [`Engine::set_delimiters("<<", ">>")`](Engine::set_delimiters)
//!   swaps `{{` / `}}` for any pair.
//!
//! ## Caching
//!
//! [`Engine::render_page`] caches results keyed by
//! `"{layout}:{Context::hash()}"`. The cache uses true LRU eviction
//! when bounded — see [`crate::cache`].
//!
//! ## Errors
//!
//! Both render entry points return `Result<String, EngineError>`.
//! Missing keys produce `EngineError::Render`; malformed templates
//! produce `EngineError::InvalidTemplate`. See [`crate::error`].
//!
//! ## Examples
//!
//! ```
//! use staticweaver::{Context, Engine};
//! use std::time::Duration;
//!
//! let engine = Engine::new("templates", Duration::from_secs(60));
//! let mut ctx = Context::new();
//! ctx.set("name".to_string(), "Ada".to_string());
//! let out = engine
//!     .render_template("Hello, {{name}}!", &ctx)
//!     .unwrap();
//! assert_eq!(out, "Hello, Ada!");
//! ```

use crate::cache::Cache;
use crate::context::Context;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Signature of a user-registered filter, as accepted by
/// [`Engine::add_filter`]. The filter receives the current pipeline
/// value as `&str` and any colon-separated arguments as `&[String]`,
/// and returns the transformed value or an `EngineError`. Wrapped in
/// an `Arc` so an `Engine` stays cheap to clone.
///
/// # Examples
///
/// ```
/// use staticweaver::engine::{Engine, FilterFn};
/// use staticweaver::EngineError;
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// let slugify: FilterFn = Arc::new(
///     |input: &str, _args: &[String]| -> Result<String, EngineError> {
///         Ok(input
///             .chars()
///             .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
///             .collect())
///     },
/// );
/// let mut engine = Engine::new("", Duration::from_secs(60));
/// engine.add_filter("slugify", slugify);
/// ```
pub type FilterFn = Arc<
    dyn Fn(&str, &[String]) -> Result<String, EngineError>
        + Send
        + Sync,
>;

/// Signature of a user-registered test, as accepted by
/// [`Engine::add_test`]. The test receives the operand value the
/// `is X` predicate is applied to and any extra arguments, and
/// returns a boolean (or an `EngineError` for malformed inputs).
/// Custom tests override built-ins (`defined`, `empty`, `none`) of
/// the same name.
///
/// # Examples
///
/// ```
/// use staticweaver::engine::{Engine, TestFn};
/// use staticweaver::context::Value;
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// let is_admin: TestFn = Arc::new(|v: &Value, _args: &[String]| {
///     Ok(matches!(v, Value::String(s) if s == "admin"))
/// });
/// let mut engine = Engine::new("", Duration::from_secs(60));
/// engine.add_test("admin", is_admin);
/// ```
pub type TestFn = Arc<
    dyn Fn(
            &crate::context::Value,
            &[String],
        ) -> Result<bool, EngineError>
        + Send
        + Sync,
>;

/// Owned name → body map collected from a child template's
/// `{{#block "name"}}…{{/block}}` declarations and consumed by the base
/// template's matching `{{#block "name"}}` tags. Owned strings sidestep
/// lifetime entanglement when blocks are merged across multiple
/// `{{#extends}}` levels.
type BlockOverrides = HashMap<String, String>;

/// Source-of-truth for template files.
///
/// The default loader ([`FsLoader`]) reads templates from a directory
/// on disk, mapping `name` to `<root>/<name>.html`. Implementing
/// this trait yourself lets you serve templates from any backend —
/// an in-memory map ([`MemoryLoader`]), a database, an embedded
/// asset bundle, or a remote service.
///
/// Loaders are looked up by `Engine` whenever it resolves a
/// `{{> partial}}`, a `{{#extends "base"}}`, or a `render_page`
/// call. The `name` passed in has already been validated against
/// path-traversal patterns (no `/`, `\`, `..`, or null bytes), so
/// implementations don't need to re-validate.
///
/// `Send + Sync` is required so an `Engine` stays usable across
/// threads — e.g. behind an `Arc<Engine>` in a web handler.
///
/// # Examples
///
/// ```
/// use staticweaver::engine::{Engine, MemoryLoader};
/// use staticweaver::{Context, EngineError};
/// use std::collections::HashMap;
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// // In-memory templates — no filesystem touched.
/// let mut store = HashMap::new();
/// let _ = store.insert("hello".to_string(), "Hi, {{name}}!".to_string());
/// let mut engine = Engine::with_loader(
///     Arc::new(MemoryLoader::new(store)),
///     Duration::from_secs(60),
/// );
///
/// let mut ctx = Context::new();
/// ctx.set("name".to_string(), "Ada".to_string());
/// assert_eq!(engine.render_page(&ctx, "hello").unwrap(), "Hi, Ada!");
/// # Ok::<_, EngineError>(())
/// ```
pub trait TemplateLoader: Send + Sync {
    /// Loads the template named `name`. Implementations should
    /// return `EngineError::Io` for missing/unreadable templates
    /// (so the existing error pattern matching keeps working).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::{MemoryLoader, TemplateLoader};
    /// let mut loader = MemoryLoader::default();
    /// let _ = loader.insert("hello", "Hi {{name}}!");
    /// assert_eq!(loader.load("hello").unwrap(), "Hi {{name}}!");
    /// assert!(loader.load("missing").is_err());
    /// ```
    fn load(&self, name: &str) -> Result<String, EngineError>;
}

/// Filesystem-backed [`TemplateLoader`]. Resolves
/// `name` to `<root>/<name>.html`. Used as the default loader by
/// [`Engine::new`].
///
/// # Examples
///
/// ```
/// use staticweaver::engine::{FsLoader, TemplateLoader};
/// use std::path::PathBuf;
///
/// let loader = FsLoader::new(PathBuf::from("templates"));
/// // loader.load("post") would read templates/post.html
/// let _ = loader; // silence unused warning
/// ```
#[derive(Debug, Clone)]
pub struct FsLoader {
    /// Directory under which `name.html` files live.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use std::path::PathBuf;
    /// let loader = FsLoader::new(PathBuf::from("templates"));
    /// assert_eq!(loader.root, PathBuf::from("templates"));
    /// ```
    pub root: PathBuf,
}

impl FsLoader {
    /// Creates a new `FsLoader` rooted at `root`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::FsLoader;
    /// use std::path::PathBuf;
    /// let loader = FsLoader::new(PathBuf::from("templates"));
    /// assert_eq!(loader.root, PathBuf::from("templates"));
    /// ```
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl TemplateLoader for FsLoader {
    fn load(&self, name: &str) -> Result<String, EngineError> {
        let path = self.root.join(format!("{name}.html"));
        Ok(fs::read_to_string(&path)?)
    }
}

/// In-memory [`TemplateLoader`] backed by a `HashMap`. Useful for
/// tests, embedded asset bundles, or any case where templates are
/// known at build time and shouldn't touch the filesystem.
///
/// # Examples
///
/// ```
/// use staticweaver::engine::{MemoryLoader, TemplateLoader};
/// use std::collections::HashMap;
///
/// let mut map = HashMap::new();
/// let _ = map.insert("greet".to_string(), "Hello!".to_string());
/// let loader = MemoryLoader::new(map);
/// assert_eq!(loader.load("greet").unwrap(), "Hello!");
/// ```
#[derive(Debug, Clone, Default)]
pub struct MemoryLoader {
    /// Template name → body map.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::MemoryLoader;
    /// let mut loader = MemoryLoader::default();
    /// let _ = loader.insert("hello", "Hi {{name}}!");
    /// assert_eq!(loader.templates.len(), 1);
    /// ```
    pub templates: HashMap<String, String>,
}

impl MemoryLoader {
    /// Creates a `MemoryLoader` populated with `templates`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::MemoryLoader;
    /// use std::collections::HashMap;
    /// let mut store = HashMap::new();
    /// let _ = store.insert("k".to_string(), "v".to_string());
    /// let loader = MemoryLoader::new(store);
    /// assert_eq!(loader.templates.len(), 1);
    /// ```
    #[must_use]
    pub fn new(templates: HashMap<String, String>) -> Self {
        Self { templates }
    }

    /// Inserts or replaces the template named `name`. Returns the
    /// previous body if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::MemoryLoader;
    /// let mut loader = MemoryLoader::default();
    /// assert_eq!(loader.insert("k", "first"), None);
    /// assert_eq!(loader.insert("k", "second").as_deref(), Some("first"));
    /// ```
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        body: impl Into<String>,
    ) -> Option<String> {
        self.templates.insert(name.into(), body.into())
    }
}

impl TemplateLoader for MemoryLoader {
    fn load(&self, name: &str) -> Result<String, EngineError> {
        self.templates.get(name).cloned().ok_or_else(|| {
            EngineError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("MemoryLoader: no template named `{name}`"),
            ))
        })
    }
}

/// Loop-control signal returned by `render_recursive`. `Continue`
/// asks the enclosing `#each` to skip to the next iteration;
/// `Break` asks it to stop iterating. `Done` is the normal
/// terminal state and bubbles up unchanged through every layer.
/// At the top level the signal is silently discarded — using
/// `#break` / `#continue` outside a loop is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowSignal {
    Done,
    Break,
    Continue,
}

/// Maximum nesting depth for `{{#extends}}`, partial inclusion, and
/// `{{#block}}` body rendering combined. Caps mutually-recursive
/// templates before the stack does.
const MAX_RENDER_DEPTH: usize = 10;

#[cfg(feature = "remote-templates")]
use std::{fs::File, path::Path};

/// Canonical engine error type. Re-exported from `crate::error` to keep a
/// single source of truth; callers can use either `staticweaver::EngineError`
/// or `staticweaver::engine::EngineError` and pattern-match interchangeably.
///
/// # Examples
///
/// ```
/// // Both paths refer to the same type.
/// fn _f(e: staticweaver::engine::EngineError)
///     -> staticweaver::EngineError { e }
/// ```
pub use crate::error::EngineError;

/// Filenames fetched by default when `Engine::create_template_folder` is
/// called with an HTTP/S URL and no explicit file list. Matches the
/// historical six-file set for backwards compatibility; callers who need
/// a different layout should use
/// [`Engine::create_template_folder_with_files`].
///
/// # Examples
///
/// ```
/// use staticweaver::engine::DEFAULT_TEMPLATE_FILES;
/// assert!(DEFAULT_TEMPLATE_FILES.contains(&"index.html"));
/// ```
pub const DEFAULT_TEMPLATE_FILES: &[&str] = &[
    "contact.html",
    "index.html",
    "page.html",
    "post.html",
    "main.js",
    "sw.js",
];

/// The main template rendering engine.
///
/// # Examples
///
/// ```
/// use staticweaver::{Context, Engine};
/// use std::time::Duration;
///
/// let engine = Engine::new("templates", Duration::from_secs(60));
/// let mut ctx = Context::new();
/// ctx.set("who".to_string(), "world".to_string());
/// let out = engine.render_template("hello {{who}}", &ctx).unwrap();
/// assert_eq!(out, "hello world");
/// ```
pub struct Engine {
    /// Path to the template directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("templates", Duration::from_secs(60));
    /// assert_eq!(engine.template_path, "templates");
    /// ```
    pub template_path: String,
    /// Cache for rendered templates keyed by `"{layout}:{ctx.hash()}"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// assert_eq!(engine.render_cache.len(), 0);
    /// ```
    pub render_cache: Cache<String, String>,
    /// Opening delimiter for template tags. Defaults to `"{{"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// assert_eq!(engine.open_delim, "{{");
    /// ```
    pub open_delim: String,
    /// Closing delimiter for template tags. Defaults to `"}}"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// assert_eq!(engine.close_delim, "}}");
    /// ```
    pub close_delim: String,
    /// When true, values substituted into templates are HTML-escaped
    /// (`&`, `<`, `>`, `"`, `'`). Prefix a key with `!` to opt out per-tag
    /// (e.g. `{{!content}}` emits the raw value).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("", Duration::from_secs(60))
    ///     .with_html_escape(false);
    /// assert!(!engine.escape_html);
    /// ```
    pub escape_html: bool,
    /// User-registered filters keyed by name. Looked up *before* the
    /// built-in filter set, so a custom filter can override a
    /// built-in of the same name. Populate via [`Engine::add_filter`].
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// let mut engine = Engine::new("", Duration::from_secs(60));
    /// let _ = engine.add_filter(
    ///     "shout",
    ///     Arc::new(|s, _| Ok(s.to_uppercase())),
    /// );
    /// assert!(engine.custom_filters.contains_key("shout"));
    /// ```
    pub custom_filters: HashMap<String, FilterFn>,
    /// User-registered tests keyed by name. Looked up *before* the
    /// built-in tests (`defined`, `empty`, `none`) so a custom test
    /// can override a built-in of the same name. Populate via
    /// [`Engine::add_test`].
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// let mut engine = Engine::new("", Duration::from_secs(60));
    /// let _ = engine.add_test("admin", Arc::new(|_v, _| Ok(true)));
    /// assert!(engine.custom_tests.contains_key("admin"));
    /// ```
    pub custom_tests: HashMap<String, TestFn>,
    /// Source of template content for `render_page`, partial
    /// includes, and `{{#extends}}`. Defaults to an [`FsLoader`]
    /// rooted at `template_path`. Override via
    /// [`Engine::with_loader`] for in-memory or custom-backend
    /// templates.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("templates", Duration::from_secs(60));
    /// // The default loader is an FsLoader; the field exposes it
    /// // as a trait object so callers can swap it freely.
    /// assert!(engine.loader.load("missing").is_err());
    /// ```
    pub loader: Arc<dyn TemplateLoader>,
    /// File extensions (e.g. `.html`, `.xml`) for which
    /// [`Engine::render_page`] auto-escapes substitutions.
    /// Empty (default) = use the global `escape_html` setting
    /// uniformly. When non-empty, layouts whose name ends with one
    /// of these extensions auto-escape; everything else renders
    /// raw. Matches Tera's per-extension autoescape behaviour.
    /// Populate via [`Engine::autoescape_on`].
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let mut engine = Engine::new("", Duration::from_secs(60));
    /// let _ = engine.autoescape_on(&[".html"]);
    /// assert_eq!(engine.autoescape_extensions, vec![".html"]);
    /// ```
    pub autoescape_extensions: Vec<String>,
}

// `Engine` is mostly auto-debuggable, but `custom_filters` carries
// `Box<dyn Fn>`-style values that are not. Print only the registered
// filter names so the rest of the struct still surfaces useful state.
impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("template_path", &self.template_path)
            .field("render_cache", &self.render_cache)
            .field("open_delim", &self.open_delim)
            .field("close_delim", &self.close_delim)
            .field("escape_html", &self.escape_html)
            .field(
                "custom_filters",
                &self.custom_filters.keys().collect::<Vec<_>>(),
            )
            .field(
                "custom_tests",
                &self.custom_tests.keys().collect::<Vec<_>>(),
            )
            // dyn-trait field — Debug just shows the placeholder.
            .field("loader", &"<dyn TemplateLoader>")
            .field("autoescape_extensions", &self.autoescape_extensions)
            .finish()
    }
}

impl Engine {
    /// Creates a new `Engine` instance with HTML escaping enabled.
    ///
    /// # Arguments
    ///
    /// * `template_path` - The path to the template directory.
    /// * `cache_ttl` - Time-to-live for cached rendered templates.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(3600));
    /// ```
    #[must_use]
    pub fn new(template_path: &str, cache_ttl: Duration) -> Self {
        Self {
            template_path: template_path.to_string(),
            render_cache: Cache::new(cache_ttl),
            open_delim: "{{".to_string(),
            close_delim: "}}".to_string(),
            escape_html: true,
            custom_filters: HashMap::new(),
            custom_tests: HashMap::new(),
            loader: Arc::new(FsLoader::new(PathBuf::from(
                template_path,
            ))),
            autoescape_extensions: Vec::new(),
        }
    }

    /// Constructs an engine that loads templates through `loader`
    /// instead of the default filesystem backend. Use this when
    /// you want in-memory templates ([`MemoryLoader`]), templates
    /// stored in a database, embedded asset bundles, or any other
    /// custom source.
    ///
    /// `template_path` is set to the empty string in this
    /// constructor — it's only used by the default `FsLoader`,
    /// which the custom loader replaces. Path-validation logic on
    /// partial / extends names still applies.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::{Engine, MemoryLoader};
    /// use staticweaver::Context;
    /// use std::collections::HashMap;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let mut store = HashMap::new();
    /// let _ = store
    ///     .insert("hello".to_string(), "Hi, {{name}}!".to_string());
    /// let mut engine = Engine::with_loader(
    ///     Arc::new(MemoryLoader::new(store)),
    ///     Duration::from_secs(60),
    /// );
    ///
    /// let mut ctx = Context::new();
    /// ctx.set("name".to_string(), "Ada".to_string());
    /// assert_eq!(
    ///     engine.render_page(&ctx, "hello").unwrap(),
    ///     "Hi, Ada!",
    /// );
    /// ```
    #[must_use]
    pub fn with_loader(
        loader: Arc<dyn TemplateLoader>,
        cache_ttl: Duration,
    ) -> Self {
        Self {
            template_path: String::new(),
            render_cache: Cache::new(cache_ttl),
            open_delim: "{{".to_string(),
            close_delim: "}}".to_string(),
            escape_html: true,
            custom_filters: HashMap::new(),
            custom_tests: HashMap::new(),
            loader,
            autoescape_extensions: Vec::new(),
        }
    }

    /// Configures per-extension auto-escape policy for
    /// [`Engine::render_page`]. When called with a non-empty list,
    /// `render_page` auto-escapes ONLY for layouts whose name ends
    /// with one of the listed suffixes; layouts with any other
    /// extension render raw.
    ///
    /// Mirrors Tera's `autoescape_on(vec![".html"])`. The global
    /// `escape_html` flag still applies to `render_template`
    /// (which has no layout name to inspect).
    ///
    /// Returns `&mut Self` for builder-style chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use staticweaver::engine::MemoryLoader;
    /// use std::collections::HashMap;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let mut store = HashMap::new();
    /// let _ = store.insert("page.html".to_string(), "{{x}}".to_string());
    /// let _ = store.insert("plain.txt".to_string(), "{{x}}".to_string());
    /// let mut engine = Engine::with_loader(
    ///     Arc::new(MemoryLoader::new(store)),
    ///     Duration::from_secs(60),
    /// );
    /// engine.autoescape_on(&[".html"]);
    /// let mut ctx = Context::new();
    /// ctx.set("x".to_string(), "<b>".to_string());
    /// // .html: escaped
    /// assert_eq!(engine.render_page(&ctx, "page.html").unwrap(), "&lt;b&gt;");
    /// // .txt: raw
    /// assert_eq!(engine.render_page(&ctx, "plain.txt").unwrap(), "<b>");
    /// ```
    pub fn autoescape_on(&mut self, extensions: &[&str]) -> &mut Self {
        self.autoescape_extensions =
            extensions.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Registers a custom filter under `name`. Custom filters are
    /// looked up *before* the built-in set, so registering a name
    /// that already exists (e.g. `uppercase`) overrides the built-in.
    /// Returns `&mut Self` for builder-style chaining.
    ///
    /// The filter receives the current pipeline value as `&str` and
    /// any colon-separated arguments as `&[String]`. See
    /// [`FilterFn`] for the full signature.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("", Duration::from_secs(60));
    /// engine.add_filter(
    ///     "shout",
    ///     Arc::new(|input, _args| Ok(format!("{}!!!", input.to_uppercase()))),
    /// );
    /// let mut ctx = Context::new();
    /// ctx.set("greeting".to_string(), "hello".to_string());
    /// let out = engine
    ///     .render_template("{{greeting | shout}}", &ctx)
    ///     .unwrap();
    /// assert_eq!(out, "HELLO!!!");
    /// ```
    pub fn add_filter(
        &mut self,
        name: &str,
        filter: FilterFn,
    ) -> &mut Self {
        let _ = self.custom_filters.insert(name.to_string(), filter);
        self
    }

    /// Registers a custom test under `name`. Custom tests are
    /// looked up *before* the built-in set (`defined`, `empty`,
    /// `none`), so registering a name that already exists overrides
    /// the built-in. Returns `&mut Self` for builder-style chaining.
    ///
    /// Used in `#if` block expressions: `{{#if value is X}}…{{/if}}`
    /// passes `value` and any colon-separated args to the test
    /// function, which returns a boolean. `is not X` flips the
    /// result via the same negation pathway that built-ins use.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use staticweaver::context::Value;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("", Duration::from_secs(60));
    /// engine.add_test(
    ///     "admin",
    ///     Arc::new(|v, _args| {
    ///         Ok(matches!(v, Value::String(s) if s == "admin"))
    ///     }),
    /// );
    /// let mut ctx = Context::new();
    /// ctx.set("role".to_string(), "admin".to_string());
    /// let out = engine
    ///     .render_template(
    ///         "{{#if role is admin}}Y{{else}}N{{/if}}",
    ///         &ctx,
    ///     )
    ///     .unwrap();
    /// assert_eq!(out, "Y");
    /// ```
    pub fn add_test(&mut self, name: &str, test: TestFn) -> &mut Self {
        let _ = self.custom_tests.insert(name.to_string(), test);
        self
    }

    /// Toggles HTML escaping for substituted values. Returns `self` for
    /// builder-style chaining. Escaping is on by default; disable it only
    /// when the engine is used to render non-HTML output or when the caller
    /// escapes values themselves.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("", Duration::from_secs(60))
    ///     .with_html_escape(false);
    /// let mut ctx = Context::new();
    /// ctx.set("body".to_string(), "<b>hi</b>".to_string());
    /// let out = engine.render_template("{{body}}", &ctx).unwrap();
    /// assert_eq!(out, "<b>hi</b>");
    /// ```
    #[must_use]
    pub fn with_html_escape(mut self, enable: bool) -> Self {
        self.escape_html = enable;
        self
    }

    /// Renders a full page using a layout file from the `template_path`.
    ///
    /// The engine automatically appends `.html` to the `layout` name.
    /// Results are cached using a combined hash of the layout name and
    /// the provided `context`.
    ///
    /// # Arguments
    ///
    /// * `context` - The data context for template substitution.
    /// * `layout` - The name of the layout file (without `.html`).
    ///
    /// # Errors
    ///
    /// Returns `EngineError::Io` if the layout file cannot be read, or
    /// `EngineError::InvalidTemplate` if the name is malformed (e.g.
    /// contains `..` traversal).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    /// use std::fs;
    /// use tempfile::TempDir;
    ///
    /// let temp = TempDir::new().unwrap();
    /// fs::write(temp.path().join("index.html"), "Hello, {{name}}!").unwrap();
    ///
    /// let mut engine = Engine::new(
    ///     temp.path().to_str().unwrap(),
    ///     Duration::from_secs(60),
    /// );
    /// let mut context = Context::new();
    /// context.set("name".to_string(), "World".to_string());
    ///
    /// let rendered = engine.render_page(&context, "index").unwrap();
    /// assert_eq!(rendered, "Hello, World!");
    /// ```
    pub fn render_page(
        &mut self,
        context: &Context,
        layout: &str,
    ) -> Result<String, EngineError> {
        // Reject any layout name that could escape the template directory.
        // Callers pass values like "post", "default", or "blog/post".
        validate_path(layout)?;

        let cache_key = format!("{}:{}", layout, context.hash());

        // Return cached result if available
        if let Some(cached) = self.render_cache.get(&cache_key) {
            return Ok(cached.to_string());
        }

        // Load the layout body via the configured TemplateLoader
        // (defaults to FsLoader rooted at `self.template_path`).
        let template_content = self.loader.load(layout)?;

        // Per-extension auto-escape policy: if the user opted in
        // via `autoescape_on(&[".html", …])`, the layout's own
        // extension decides whether substitutions get escaped, not
        // the global `escape_html` flag. Save / restore the flag
        // around the render so the engine state stays clean for
        // subsequent calls (and so render_template — which has no
        // layout name — continues to use the global setting).
        let saved_escape = self.escape_html;
        if !self.autoescape_extensions.is_empty() {
            self.escape_html = self
                .autoescape_extensions
                .iter()
                .any(|ext| layout.ends_with(ext.as_str()));
        }

        // Render the template with the provided context.
        let rendered = self.render_template(&template_content, context);

        // Restore the global flag before propagating the result so
        // an error mid-render doesn't leak the override.
        self.escape_html = saved_escape;
        let rendered = rendered?;

        // Cache the rendered result for future use
        let _ = self.render_cache.insert(cache_key, rendered.clone());

        Ok(rendered)
    }

    /// Renders a raw template string against the provided `context`.
    ///
    /// Supports:
    ///   - `{{ key }}`: Substitution (HTML escaped by default).
    ///   - `{{!key}}`: Raw substitution (no escaping).
    ///   - `{{> partial}}`: Recursive partial inclusion.
    ///   - `{{#if key}}...{{else}}...{{/if}}`: Conditionals.
    ///   - `{{#each list}}...{{/each}}`: Iteration.
    ///
    /// # Arguments
    ///
    /// * `template` - The raw string containing template tags.
    /// * `context` - The data context.
    ///
    /// # Errors
    ///
    /// Returns `EngineError::InvalidTemplate` for syntax errors or
    /// `EngineError::Render` for unresolved tags or filter errors.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set_value("items".to_string(), vec!["a", "b"]);
    ///
    /// let out = engine.render_template(
    ///     "{{#each items}}{{this}} {{/each}}",
    ///     &ctx
    /// ).unwrap();
    /// assert_eq!(out, "a b ");
    /// ```
    pub fn render_template(
        &self,
        template: &str,
        context: &Context,
    ) -> Result<String, EngineError> {
        if template.trim().is_empty() {
            return Err(EngineError::InvalidTemplate(
                "Template is empty".to_string(),
            ));
        }

        let mut output = String::with_capacity(template.len());
        // Top-level FlowSignal is discarded — `#break` / `#continue`
        // outside a loop is a no-op.
        let _ = self.render_resolved(
            template,
            template,
            context,
            BlockOverrides::new(),
            &mut output,
            0,
        )?;
        Ok(output)
    }

    /// Renders `template` against `context` and writes the result
    /// directly to `writer`. Convenience wrapper for callers that want
    /// to stream into a `Vec<u8>`, an HTTP response body, a file, or
    /// any other [`std::io::Write`] sink without managing the
    /// intermediate `String` themselves.
    ///
    /// Equivalent to `writer.write_all(engine.render_template(t, c)?
    /// .as_bytes())` with two differences: I/O failures map cleanly
    /// to `EngineError::Io`, and a future zero-copy variant could
    /// land here without changing the call site.
    ///
    /// # Note
    ///
    /// The implementation still allocates one `String` internally.
    /// The whitespace-control trim (`{{- ... -}}`) needs lookback
    /// into the rendered buffer, which `io::Write` cannot provide.
    /// True zero-copy streaming would require either dropping
    /// `{{- -}}` support or buffering the would-be-trimmed bytes;
    /// neither is worth the API churn today.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set("name".to_string(), "Ada".to_string());
    ///
    /// let mut buf: Vec<u8> = Vec::new();
    /// engine
    ///     .render_to("Hello, {{name}}!", &ctx, &mut buf)
    ///     .unwrap();
    /// assert_eq!(buf, b"Hello, Ada!");
    /// ```
    pub fn render_to<W: Write>(
        &self,
        template: &str,
        context: &Context,
        writer: &mut W,
    ) -> Result<(), EngineError> {
        let rendered = self.render_template(template, context)?;
        writer.write_all(rendered.as_bytes())?;
        Ok(())
    }

    /// File-backed counterpart to [`Engine::render_to`]. Looks up
    /// `layout` in `template_path` (with `.html` appended), renders
    /// the page, and writes the result to `writer`. Caches the
    /// rendered output by `(layout, ctx.hash())` like
    /// [`Engine::render_page`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    /// use std::fs::File;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(60));
    /// let ctx = Context::new();
    ///
    /// let mut out = File::create("/tmp/page.html").unwrap();
    /// engine.render_page_to(&ctx, "index", &mut out).unwrap();
    /// ```
    pub fn render_page_to<W: Write>(
        &mut self,
        context: &Context,
        layout: &str,
        writer: &mut W,
    ) -> Result<(), EngineError> {
        let rendered = self.render_page(context, layout)?;
        writer.write_all(rendered.as_bytes())?;
        Ok(())
    }

    /// Resolves any `{{#extends "base"}}` chain on `template` before
    /// rendering. Each level's `{{#block "name"}}…{{/block}}`
    /// declarations are collected and merged into `accumulated`; child
    /// definitions win over parent (or_insert preserves existing).
    /// Once a template that does not extend anything is reached, the
    /// fully-merged overrides are handed to `render_recursive` for the
    /// real render.
    fn render_resolved(
        &self,
        origin: &str,
        template: &str,
        context: &Context,
        mut accumulated: BlockOverrides,
        output: &mut String,
        depth: usize,
    ) -> Result<FlowSignal, EngineError> {
        if depth > MAX_RENDER_DEPTH {
            return Err(EngineError::Render(format!(
                "Maximum template recursion depth ({MAX_RENDER_DEPTH}) exceeded"
            )));
        }
        let open = self.open_delim.as_str();
        let close = self.close_delim.as_str();

        match parse_extends(template, open, close)? {
            Some(base_name) => {
                validate_path(base_name)?;
                for (k, v) in collect_blocks(template, open, close)? {
                    let _ = accumulated.entry(k).or_insert(v);
                }
                let base_content = self.loader.load(base_name)?;
                // Switch `origin` to the base file's content — line
                // numbers in errors reported during base-template
                // rendering refer to the base file, not the child.
                self.render_resolved(
                    &base_content,
                    &base_content,
                    context,
                    accumulated,
                    output,
                    depth + 1,
                )
            }
            None => self.render_recursive(
                origin,
                template,
                context,
                &accumulated,
                None,
                output,
                depth,
            ),
        }
    }

    /// Recursive rendering core. Walks `template`, dispatching on tag
    /// shape:
    ///
    ///   - `{{ key }}`             — substitute a value
    ///   - `{{!key}}`              — substitute without HTML escape
    ///   - `{{> partial}}`         — include and render another template
    ///   - `{{#if key}}…{{/if}}`   — conditional block (optional `{{else}}`)
    ///   - `{{#each list}}…{{/each}}` — iterate a `Value::List`, binding
    ///     each element to `this`
    ///   - `{{#block "name"}}…{{/block}}` — substitute the override from
    ///     `blocks` if present, otherwise fall back to the default body
    ///
    /// Block bodies are rendered through this same function, so escaping,
    /// dot-notation, and nested control flow compose without duplication.
    fn render_recursive(
        &self,
        origin: &str,
        template: &str,
        context: &Context,
        blocks: &BlockOverrides,
        super_body: Option<&str>,
        output: &mut String,
        depth: usize,
    ) -> Result<FlowSignal, EngineError> {
        if depth > MAX_RENDER_DEPTH {
            return Err(EngineError::Render(format!(
                "Maximum template recursion depth ({MAX_RENDER_DEPTH}) exceeded"
            )));
        }

        let open = self.open_delim.as_str();
        let close = self.close_delim.as_str();
        let mut rest = template;
        // Local scope for `{{#set k = v}}`. Materialised lazily on the
        // first set; subsequent lookups in this scope read from `local`
        // (rebound below as `active`). Recursive descent into `#if` /
        // `#each` / `#block` / partial bodies inherits whatever the
        // caller's scope had at that moment.
        let mut local: Option<Context> = None;

        while let Some(start) = rest.find(open) {
            // Active context for this iteration: the local scope if any
            // `#set` has happened at this level, otherwise the parent.
            let active: &Context = local.as_ref().unwrap_or(context);

            // Count the run of backslashes immediately preceding `start`.
            // An odd count leaves one backslash active -> the delimiter
            // is escaped (emitted literally, no tag lookup). An even
            // count means every backslash is paired and cancels; the
            // delimiter is a real tag opener.
            let bytes = rest.as_bytes();
            let mut bs = 0usize;
            while start > bs && bytes[start - bs - 1] == b'\\' {
                bs += 1;
            }
            let text_end = start - bs;
            output.push_str(&rest[..text_end]);
            for _ in 0..bs / 2 {
                output.push('\\');
            }
            if bs % 2 == 1 {
                output.push_str(open);
                rest = &rest[start + open.len()..];
                continue;
            }

            let after_open = &rest[start + open.len()..];
            let end = after_open.find(close).ok_or_else(|| {
                EngineError::InvalidTemplate(format!(
                    "Unclosed template tag{}",
                    pos_suffix(origin, &rest[start..])
                ))
            })?;
            let key_raw = &after_open[..end];

            if key_raw.contains(open) {
                return Err(EngineError::InvalidTemplate(format!(
                    "Nested delimiters are not allowed{}",
                    pos_suffix(origin, key_raw)
                )));
            }

            // Whitespace control:
            //   {{- ... }}  strips trailing whitespace from `output`.
            //   {{ ... -}}  skips leading whitespace in the next chunk.
            //   {{- ... -}} does both.
            // The dashes must be the first / last non-whitespace bytes
            // inside the tag; `{{ - key - }}` (space-padded) is *not* a
            // whitespace marker — it parses as the key string `- key -`,
            // which would error as unresolved.
            //
            // Block comments `{{!-- ... --}}` are exempt from whitespace
            // control because their closing marker literally is `--`,
            // which would otherwise be mis-detected as a strip-right.
            // Inline comments still compose with stripping via
            // `{{- ! note -}}`.
            let mut key_trimmed: &str = key_raw.trim();
            let is_block_comment = key_trimmed.starts_with("!--");
            let strip_left =
                !is_block_comment && key_trimmed.starts_with('-');
            if strip_left {
                key_trimmed = key_trimmed[1..].trim_start();
                let kept = output.trim_end().len();
                output.truncate(kept);
            }
            let strip_right =
                !is_block_comment && key_trimmed.ends_with('-');
            if strip_right {
                key_trimmed =
                    key_trimmed[..key_trimmed.len() - 1].trim_end();
            }

            let after_tag_raw = &after_open[end + close.len()..];
            let after_tag = if strip_right {
                after_tag_raw.trim_start()
            } else {
                after_tag_raw
            };

            // ── Block dispatch ──────────────────────────────────────
            if let Some(arg) = key_trimmed.strip_prefix("#if") {
                let arg = arg.trim();
                let (body, after_block) =
                    extract_block(after_tag, "if", open, close)
                        .map_err(|e| {
                            annotate_pos(e, origin, key_trimmed)
                        })?;
                let (then_body, else_body) =
                    split_else(body, open, close);
                // Parse `arg` as an expression (currently bare path or
                // `lhs OP rhs` comparison). A bare path keeps the legacy
                // truthiness semantics; a comparison evaluates to Bool
                // and `is_truthy` agrees with it.
                let cond = parse_expr(arg)
                    .map_err(|e| annotate_pos(e, origin, arg))?
                    .eval(active, self)
                    .map_err(|e| annotate_pos(e, origin, arg))?
                    .is_truthy();
                let chosen = if cond {
                    then_body
                } else {
                    else_body.unwrap_or("")
                };
                if !chosen.is_empty() {
                    let signal = self.render_recursive(
                        origin,
                        chosen,
                        active,
                        blocks,
                        super_body,
                        output,
                        depth + 1,
                    )?;
                    // Propagate Break/Continue upward; the
                    // enclosing #each (if any) handles it.
                    if signal != FlowSignal::Done {
                        return Ok(signal);
                    }
                }
                rest = after_block;
                continue;
            }

            if let Some(arg) = key_trimmed.strip_prefix("#each") {
                let arg = arg.trim();
                let (body, after_block) =
                    extract_block(after_tag, "each", open, close)
                        .map_err(|e| {
                            annotate_pos(e, origin, key_trimmed)
                        })?;

                // Range form: `#each START..END` (END exclusive).
                // Both sides are full expressions, so paths and
                // arithmetic both work: `#each 0..items.length`,
                // `#each (a)..(b+1)`. The `..` separator is
                // unambiguous because identifiers can't contain
                // consecutive dots.
                let range_items: Option<Vec<crate::context::Value>> =
                    if let Some((lo_str, hi_str)) = arg.split_once("..")
                    {
                        let lo = parse_expr(lo_str.trim())
                            .map_err(|e| annotate_pos(e, origin, arg))?
                            .eval(active, self)
                            .map_err(|e| {
                                annotate_pos(e, origin, arg)
                            })?;
                        let hi = parse_expr(hi_str.trim())
                            .map_err(|e| annotate_pos(e, origin, arg))?
                            .eval(active, self)
                            .map_err(|e| {
                                annotate_pos(e, origin, arg)
                            })?;
                        let (lo_n, hi_n) = match (lo, hi) {
                            (
                                crate::context::Value::Number(a),
                                crate::context::Value::Number(b),
                            ) => (a, b),
                            (l, h) => {
                                return Err(
                                    EngineError::InvalidTemplate(
                                        format!(
                                        "#each range bounds must be \
                                         numbers, got {l:?}..{h:?}{}",
                                        pos_suffix(origin, arg)
                                    ),
                                    ),
                                );
                            }
                        };
                        Some(
                            (lo_n..hi_n)
                                .map(crate::context::Value::Number)
                                .collect(),
                        )
                    } else {
                        None
                    };

                // Iterate Lists by position (binds @index/@first/
                // @last) and Maps by key (also binds @key). Sort Map
                // entries by key so iteration order is deterministic
                // across runs — FnvHashMap iteration order is
                // otherwise unspecified.
                let entries: Vec<(
                    Option<String>,
                    &crate::context::Value,
                )> = if let Some(range) = range_items.as_ref() {
                    range.iter().map(|v| (None, v)).collect()
                } else {
                    let target =
                        active.get_path(arg).ok_or_else(|| {
                            EngineError::Render(format!(
                                "#each: unresolved list `{arg}`{}",
                                pos_suffix(origin, arg)
                            ))
                        })?;
                    match target {
                        crate::context::Value::List(items) => {
                            items.iter().map(|v| (None, v)).collect()
                        }
                        crate::context::Value::Map(map) => {
                            let mut keyed: Vec<_> =
                                map.iter().collect();
                            keyed.sort_by(|a, b| a.0.cmp(b.0));
                            keyed
                                .into_iter()
                                .map(|(k, v)| (Some(k.clone()), v))
                                .collect()
                        }
                        other => {
                            return Err(EngineError::InvalidTemplate(
                                format!(
                                    "#each expects a list or map, got {other:?}"
                                ),
                            ));
                        }
                    }
                };

                let total = entries.len();
                // Clone the active context ONCE, then mutate it in
                // place across iterations. The previous code cloned
                // per iteration — for each_1000, that was 1000 full
                // Context clones. Loop variables (`this`, `@index`,
                // `@first`, `@last`, `@key`) overwrite the same slots
                // every iteration; nested `#set` writes to a local
                // scope inside `render_recursive` and does not leak
                // back into `child`, so cloning once is sound.
                let mut child = active.clone();
                for (index, (key_opt, item)) in
                    entries.iter().enumerate()
                {
                    // `set_value_str` reuses the existing key slot
                    // after the first iteration — saves one String
                    // allocation per loop variable per iteration
                    // (5 × N for List with @key, 4 × N otherwise).
                    // For `Value::String` items the dedicated
                    // set_value_string variant additionally reuses
                    // the destination String's heap buffer instead
                    // of cloning, eliminating the per-iter alloc
                    // for the most common loop-item shape.
                    match item {
                        crate::context::Value::String(s) => {
                            child.set_value_string("this", s);
                        }
                        other => {
                            child.set_value_str(
                                "this",
                                (*other).clone(),
                            );
                        }
                    }
                    child.set_value_str(
                        "@index",
                        i64::try_from(index).unwrap_or(i64::MAX),
                    );
                    child.set_value_str("@first", index == 0);
                    child.set_value_str("@last", index + 1 == total);
                    if let Some(k) = key_opt {
                        child.set_value_str("@key", k.as_str());
                    }
                    let signal = self.render_recursive(
                        origin,
                        body,
                        &child,
                        blocks,
                        super_body,
                        output,
                        depth + 1,
                    )?;
                    // `#each` is the loop-control sink:
                    //   * Break  -> stop iterating, render normally
                    //              after the loop.
                    //   * Continue -> skip to next iteration.
                    //   * Done   -> normal completion of this body.
                    match signal {
                        FlowSignal::Break => break,
                        FlowSignal::Continue | FlowSignal::Done => {}
                    }
                }
                rest = after_block;
                continue;
            }

            // ── Loop control ───────────────────────────────────────
            // `{{#break}}` and `{{#continue}}` short-circuit the
            // current iteration of the enclosing `#each` loop.
            // They emit nothing to the output and propagate a
            // FlowSignal up through any nested #if / #block until
            // the loop catches them. Outside a loop the signal
            // bubbles to the top-level renderer and is silently
            // discarded — using `#break` at top-level is a no-op.
            if key_trimmed == "#break" {
                return Ok(FlowSignal::Break);
            }
            if key_trimmed == "#continue" {
                return Ok(FlowSignal::Continue);
            }

            // ── Inheritance super() ────────────────────────────────
            // `{{ super() }}` inside a child #block override expands
            // to the parent block's default body, rendered through
            // the same dispatch loop so it composes naturally with
            // partials, expressions, etc. Outside an override
            // context (super_body is None) the tag emits nothing —
            // template authors who use it elsewhere get a silent
            // no-op rather than an error, matching Jinja's lenient
            // semantics.
            if key_trimmed == "super()" {
                if let Some(parent) = super_body {
                    let signal = self.render_recursive(
                        origin,
                        parent,
                        context,
                        blocks,
                        // super inside super is a no-op — the
                        // parent has no further parent here.
                        None,
                        output,
                        depth + 1,
                    )?;
                    if signal != FlowSignal::Done {
                        return Ok(signal);
                    }
                }
                rest = after_tag;
                continue;
            }

            // ── Variable assignment ────────────────────────────────
            // `{{#set name = literal}}` binds `name` in a local scope
            // visible to subsequent tags at this depth (and to any
            // recursive descent into block bodies, partials, etc.).
            // The parent context is not mutated.
            //
            // Literals: quoted strings, integers, `true`/`false`/`null`,
            // or barewords (treated as a literal string).
            if let Some(rest_set) = key_trimmed.strip_prefix("#set") {
                let (name, value) = parse_set_assignment(
                    rest_set.trim(),
                )
                .map_err(|e| annotate_pos(e, origin, key_trimmed))?;
                if local.is_none() {
                    local = Some(active.clone());
                }
                if let Some(ctx) = local.as_mut() {
                    ctx.set_value(name, value);
                }
                rest = after_tag;
                continue;
            }

            // ── Block placeholder ──────────────────────────────────
            // `{{#block "name"}}default{{/block}}` substitutes the
            // override from `blocks` if present, otherwise renders the
            // default body. Nested blocks compose: when an outer block
            // falls back to its default and that default contains
            // another `{{#block "inner"}}`, the inner override (if any)
            // still applies because the recursive call inherits the
            // same `blocks` map.
            if let Some(name_part) = key_trimmed.strip_prefix("#block")
            {
                let name = parse_block_name(name_part.trim()).map_err(
                    |e| annotate_pos(e, origin, key_trimmed),
                )?;
                let (default_body, after_block) =
                    extract_block(after_tag, "block", open, close)
                        .map_err(|e| {
                            annotate_pos(e, origin, key_trimmed)
                        })?;
                // If the child overrode this block, render the
                // override and expose the parent's default body via
                // `{{ super() }}` inside the override. If no
                // override is present, render the default and clear
                // any inherited super (super() at this level should
                // not leak the outer super body).
                let (body_to_render, super_for_body) =
                    match blocks.get(name) {
                        Some(override_body) => {
                            (override_body.as_str(), Some(default_body))
                        }
                        None => (default_body, None),
                    };
                let signal = self.render_recursive(
                    origin,
                    body_to_render,
                    context,
                    blocks,
                    super_for_body,
                    output,
                    depth + 1,
                )?;
                if signal != FlowSignal::Done {
                    return Ok(signal);
                }
                rest = after_block;
                continue;
            }

            // ── Partial inclusion ──────────────────────────────────
            // `{{> name}}`               include with current context
            // `{{> name k="v" n=7 }}`    include with overridden bindings
            //
            // `k=v` pairs are layered onto a clone of the parent context
            // so callers can pass overrides without polluting the
            // surrounding scope. Values may be quoted strings (single or
            // double), bare integers, or `true`/`false`/`null`.
            if let Some(after_arrow) = key_trimmed.strip_prefix('>') {
                let (name, params_str) =
                    split_partial_invocation(after_arrow.trim());
                if name.is_empty() {
                    return Err(EngineError::InvalidTemplate(
                        "Empty partial name".to_string(),
                    ));
                }
                validate_path(name)?;

                let content = self.loader.load(name)?;

                let render_ctx;
                let ctx_ref = if params_str.is_empty() {
                    active
                } else {
                    let mut child = active.clone();
                    for (k, v) in parse_partial_params(params_str)
                        .map_err(|e| {
                            annotate_pos(e, origin, key_trimmed)
                        })?
                    {
                        child.set_value(k, v);
                    }
                    render_ctx = child;
                    &render_ctx
                };

                // Partial bodies are NEW &str values not in the
                // outer origin — switch origin to the partial's
                // content so any error inside the partial reports a
                // line/column relative to the partial file. Loop
                // control (#break / #continue) inside a partial does
                // *not* leak across the partial boundary; the signal
                // is swallowed so partials remain self-contained.
                let _ = self.render_recursive(
                    &content,
                    &content,
                    ctx_ref,
                    blocks,
                    // super() does not cross the partial boundary;
                    // a partial that uses {{ super() }} would be a
                    // template-author error.
                    None,
                    output,
                    depth + 1,
                )?;
                rest = after_tag;
                continue;
            }

            // Stray closing / else markers at top level are a template
            // authoring error.
            if key_trimmed.starts_with("/if")
                || key_trimmed.starts_with("/each")
                || key_trimmed == "else"
            {
                return Err(EngineError::InvalidTemplate(format!(
                    "unexpected `{key_trimmed}` outside a block{}",
                    pos_suffix(origin, key_trimmed)
                )));
            }

            // ── Comments ────────────────────────────────────────────
            // `{{! ... }}` and `{{!-- ... --}}` emit nothing. We
            // disambiguate from the existing `{{!key}}` raw-substitution
            // form by requiring the bang to be followed by whitespace,
            // a `--` block-comment marker, or end-of-tag. `{{!key}}`
            // (bang immediately followed by an identifier byte) keeps
            // its raw-opt-out meaning.
            if let Some(after_bang) = key_trimmed.strip_prefix('!') {
                let is_comment = after_bang.is_empty()
                    || after_bang
                        .starts_with(|c: char| c.is_whitespace())
                    || (after_bang.starts_with("--")
                        && after_bang.ends_with("--"));
                if is_comment {
                    rest = after_tag;
                    continue;
                }
            }

            // ── Plain substitution & Filters ────────────────────────
            let (lookup_raw, raw) = match key_trimmed.strip_prefix('!')
            {
                Some(stripped) => (stripped.trim_start(), true),
                None => (key_trimmed, false),
            };

            // Split lookup from filters: `title | uppercase`,
            // `desc | truncate:50`, `name | replace:"a","b" | lowercase`.
            let mut parts = lookup_raw.split('|');
            let lookup = parts.next().unwrap_or("").trim();
            let filters: Vec<(String, Vec<String>)> = parts
                .map(parse_filter)
                .filter(|(name, _)| !name.is_empty())
                .collect();

            let value = active.get_path(lookup).ok_or_else(|| {
                EngineError::Render(format!(
                    "Unresolved template tag: {lookup}{}",
                    pos_suffix(origin, lookup)
                ))
            })?;
            let mut rendered = value.to_string();

            // A trailing `safe` filter marks the value as already-safe
            // HTML and suppresses the engine's auto-escape. Mirrors the
            // `{{!key}}` raw opt-out but composes inside a filter chain.
            let marked_safe = filters
                .last()
                .map_or(false, |(name, _)| name == "safe");

            for (name, args) in &filters {
                // The `json` filter (feature `json`) needs the
                // structured Value, not its Display string — so for
                // non-primitive types (List/Map) we'd otherwise see
                // an empty input. Special-case it at the head of
                // the dispatch chain.
                #[cfg(feature = "json")]
                if name == "json" {
                    rendered = json_encode_value(value)?;
                    continue;
                }
                // Custom filters take precedence over built-ins, so a
                // user can override e.g. `uppercase` with their own
                // locale-aware implementation.
                rendered = if let Some(custom) =
                    self.custom_filters.get(name.as_str())
                {
                    custom(&rendered, args)?
                } else {
                    apply_filter(name, args, rendered)?
                };
            }

            if raw || marked_safe || !self.escape_html {
                output.push_str(&rendered);
            } else {
                escape_html_into(&rendered, output);
            }

            rest = after_tag;
        }

        output.push_str(rest);
        Ok(FlowSignal::Done)
    }

    /// Changes the delimiters used to identify template tags.
    ///
    /// Default is `{{` and `}}`.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("t", Duration::from_secs(60));
    /// engine.set_delimiters("[[", "]]");
    ///
    /// let mut ctx = Context::new();
    /// ctx.set("k".to_string(), "v".to_string());
    /// assert_eq!(engine.render_template("[[k]]", &ctx).unwrap(), "v");
    /// ```
    pub fn set_delimiters(&mut self, open: &str, close: &str) {
        self.open_delim = open.to_string();
        self.close_delim = close.to_string();
    }

    /// Limits the number of rendered pages held in the memory cache.
    ///
    /// If the cache currently exceeds `size`, it will be cleared.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("t", Duration::from_secs(60));
    /// engine.set_max_cache_size(10);
    /// ```
    pub fn set_max_cache_size(&mut self, size: usize) {
        self.render_cache.set_capacity(size);
    }

    /// Drops all entries from the internal rendering cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("t", Duration::from_secs(60));
    /// engine.clear_cache();
    /// ```
    pub fn clear_cache(&mut self) {
        self.render_cache.clear();
    }

    /// Prepares a local directory for template storage.
    ///
    /// If `template_path` is a local directory, it returns the absolute
    /// path if it exists. If it's a URL, and the `remote-templates`
    /// feature is enabled, the engine will attempt to fetch a standard set
    /// of template files ([`DEFAULT_TEMPLATE_FILES`]).
    ///
    /// # Arguments
    ///
    /// * `template_path` - Local path or URL source.
    ///
    /// # Errors
    ///
    /// - `EngineError::Io`: the directory does not exist.
    /// - `EngineError::InvalidTemplate`: `template_path` is `None`, or a URL
    ///   was supplied without the `remote-templates` feature.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(60));
    /// // `None` is rejected — callers must pass a path or URL explicitly.
    /// assert!(engine.create_template_folder(None).is_err());
    /// ```
    pub fn create_template_folder(
        &self,
        template_path: Option<&str>,
    ) -> Result<String, EngineError> {
        self.create_template_folder_with_files(
            template_path,
            DEFAULT_TEMPLATE_FILES,
        )
    }

    /// Same as [`create_template_folder`](Self::create_template_folder) but
    /// accepts a caller-supplied list of filenames to download when
    /// `template_path` is a URL. Useful when the default
    /// [`DEFAULT_TEMPLATE_FILES`] set does not match the remote layout.
    ///
    /// `files` is ignored for local-directory paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(60));
    /// // Pass a custom filename list (e.g. just index.html).
    /// let _ = engine.create_template_folder_with_files(Some("."), &["index.html"]);
    /// ```
    pub fn create_template_folder_with_files(
        &self,
        template_path: Option<&str>,
        files: &[&str],
    ) -> Result<String, EngineError> {
        let path = template_path.ok_or_else(|| {
            EngineError::InvalidTemplate(
                "template_path is required; pass a local directory or URL"
                    .to_string(),
            )
        })?;

        if is_url(path) {
            #[cfg(feature = "remote-templates")]
            {
                if files.is_empty() {
                    return Err(EngineError::InvalidTemplate(
                        "files list must not be empty for URL fetches"
                            .to_string(),
                    ));
                }
                let dir = Self::download_files_from_url(path, files)?;
                return dir.to_str().map(str::to_string).ok_or_else(
                    || {
                        EngineError::Io(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid UTF-8 sequence in template path",
                        ))
                    },
                );
            }
            #[cfg(not(feature = "remote-templates"))]
            {
                let _ = files; // Silence unused-arg warning.
                return Err(EngineError::InvalidTemplate(
                    "remote template URLs require the `remote-templates` feature"
                        .to_string(),
                ));
            }
        }

        let _ = files;
        let current_dir = std::env::current_dir()?;
        let local_path = current_dir.join(path);
        if local_path.exists() && local_path.is_dir() {
            local_path.to_str().map(str::to_string).ok_or_else(|| {
                EngineError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid UTF-8 sequence in template path",
                ))
            })
        } else {
            Err(EngineError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Template directory not found: {path}"),
            )))
        }
    }

    /// Downloads each filename in `files` from `url` into a fresh
    /// temporary directory and returns its path. The temp directory is
    /// owned by the caller via `TempDir` and will be cleaned up on drop.
    #[cfg(feature = "remote-templates")]
    #[cfg_attr(docsrs, doc(cfg(feature = "remote-templates")))]
    fn download_files_from_url(
        url: &str,
        files: &[&str],
    ) -> Result<PathBuf, EngineError> {
        let dir = tempfile::tempdir()?;
        // `keep` (stable replacement for the deprecated `into_path`) returns
        // a PathBuf and suppresses cleanup; we accept that here because the
        // caller treats the downloaded template dir as long-lived.
        let template_dir_path = dir.keep();

        for file in files {
            Self::download_file(url, file, &template_dir_path)?;
        }

        Ok(template_dir_path)
    }

    /// Downloads a single file from `url/file` into `dir`, with a 10s
    /// timeout, an HTTP status check, and a 1 MiB body cap so a hostile or
    /// misconfigured server cannot exhaust memory.
    #[cfg(feature = "remote-templates")]
    #[cfg_attr(docsrs, doc(cfg(feature = "remote-templates")))]
    fn download_file(
        url: &str,
        file: &str,
        dir: &Path,
    ) -> Result<(), EngineError> {
        /// Per-file body cap. Template assets are HTML/JS/CSS; a megabyte is
        /// far above any realistic payload.
        const MAX_BYTES: usize = 1024 * 1024;

        let file_url = format!("{url}/{file}");
        let file_path = dir.join(file);

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&file_url)
            .timeout(Duration::from_secs(10))
            .send()?;

        if !response.status().is_success() {
            return Err(EngineError::Render(format!(
                "Failed to download {file}: HTTP {}",
                response.status()
            )));
        }

        // The downloader targets template assets (HTML/CSS/JS). Reject
        // anything whose Content-Type does not look textual before we
        // bother reading the body — stops us silently writing a binary
        // payload to disk and failing much later inside the parser.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let looks_textual = content_type.starts_with("text/")
            || content_type.starts_with("application/javascript")
            || content_type.starts_with("application/json")
            || content_type.starts_with("application/xhtml")
            || content_type.is_empty(); // some servers omit the header
        if !looks_textual {
            return Err(EngineError::Render(format!(
                "{file} has unexpected Content-Type: {content_type}"
            )));
        }

        if let Some(len) = response.content_length() {
            if len as usize > MAX_BYTES {
                return Err(EngineError::Render(format!(
                    "{file} too large: Content-Length {len} exceeds {MAX_BYTES}"
                )));
            }
        }

        let bytes = response.bytes()?;
        if bytes.len() > MAX_BYTES {
            return Err(EngineError::Render(format!(
                "{file} too large after read: {} bytes exceeds {MAX_BYTES}",
                bytes.len()
            )));
        }

        let mut out = File::create(&file_path)?;
        out.write_all(&bytes)?;
        Ok(())
    }
}

/// Utility function to check if a given path is a URL.
///
/// # Arguments
///
/// * `path` - The path to check.
///
/// # Returns
///
/// `true` if the path is a URL, `false` otherwise.
fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// Validates that a template or partial name is safe and does not
/// attempt to escape the template directory.
///
/// Allows alphanumeric characters, hyphens, underscores, and forward
/// slashes (for subdirectories). Rejects absolute paths, null bytes,
/// and `..` segments.
fn validate_path(path: &str) -> Result<(), EngineError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\0')
        || path.split(['/', '\\']).any(|seg| seg == "..")
    {
        return Err(EngineError::InvalidTemplate(format!(
            "invalid template or partial name: {path:?}"
        )));
    }
    Ok(())
}

/// Locates the body and the byte index following the matching closer for
/// a `#if` / `#each` block. `template` is positioned immediately after the
/// opener — i.e. the first character of the body.
///
/// Walks the template counting block depth so that nested `#if` inside
/// `#each` (and vice versa) match correctly. Returns `(body, after_close)`
/// where `body` is the text between the opener and the matching closer
/// and `after_close` is the substring beginning immediately after the
/// closing tag.
fn extract_block<'a>(
    template: &'a str,
    block: &str,
    open: &str,
    close: &str,
) -> Result<(&'a str, &'a str), EngineError> {
    let mut depth: usize = 1;
    let mut cursor = 0usize;
    while let Some(rel) = template[cursor..].find(open) {
        let abs = cursor + rel;
        let after_open = &template[abs + open.len()..];
        let end = after_open.find(close).ok_or_else(|| {
            EngineError::InvalidTemplate(
                "Unclosed template tag".to_string(),
            )
        })?;
        // Parse whitespace-control flags so `{{- /if -}}` matches `/if`
        // and trims body / after-block whitespace accordingly.
        let (inner, strip_l, strip_r) =
            parse_ws_control(after_open[..end].trim());
        let tag_end = abs + open.len() + end + close.len();

        if inner.starts_with("#if")
            || inner.starts_with("#each")
            || inner.starts_with("#block")
        {
            depth += 1;
        // Avoid `inner == format!("/{block}")` which would allocate
        // a String on every tag scan. The strip_prefix + equality
        // comparison is allocation-free and clippy-clean.
        } else if inner.strip_prefix('/') == Some(block) {
            depth -= 1;
            if depth == 0 {
                let body_raw = &template[..abs];
                let body = if strip_l {
                    body_raw.trim_end()
                } else {
                    body_raw
                };
                let after_raw = &template[tag_end..];
                let after = if strip_r {
                    after_raw.trim_start()
                } else {
                    after_raw
                };
                return Ok((body, after));
            }
        } else if inner.starts_with("/if")
            || inner.starts_with("/each")
            || inner.starts_with("/block")
        {
            // Closer for a different block type — must come from an
            // inner depth, decrement accordingly.
            depth -= 1;
        }
        cursor = tag_end;
    }
    Err(EngineError::InvalidTemplate(format!(
        "Unclosed `{{{{#{block}}}}}` block"
    )))
}

/// Parses a single filter spec from the right-hand side of a `|` chain.
/// Accepts `name`, `name:arg`, or `name:arg1,arg2,...`. Arguments may be
/// quoted with single or double quotes (so commas can appear inside an
/// arg). Returns `(name, args)` with surrounding whitespace removed.
/// Returns the (1-based) line and column of `slice` within `origin`,
/// or `None` if `slice` is not a substring of `origin` (which would
/// happen for partials loaded from a different file). Implemented
/// via pointer arithmetic — works because every &str the renderer
/// produces is sliced from the original template, never freshly
/// allocated.
fn position_in(origin: &str, slice: &str) -> Option<(usize, usize)> {
    let o_start = origin.as_ptr() as usize;
    let s_start = slice.as_ptr() as usize;
    let o_end = o_start.checked_add(origin.len())?;
    if s_start < o_start || s_start > o_end {
        return None;
    }
    let byte_pos = s_start - o_start;
    let head = origin.get(..byte_pos)?;
    let line = head.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match head.rfind('\n') {
        Some(p) => byte_pos - p,
        None => byte_pos + 1,
    };
    Some((line, col))
}

/// Returns ` at line N, column M` for a slice that lives inside
/// `origin`, or the empty string if the slice is unrelated. Append
/// to user-facing error messages — the caller sees a useful pointer
/// when the renderer can compute one, and gets the legacy bare
/// message when it can't (cross-file partials, synthetic strings).
fn pos_suffix(origin: &str, slice: &str) -> String {
    match position_in(origin, slice) {
        Some((line, col)) => {
            format!(" at line {line}, column {col}")
        }
        None => String::new(),
    }
}

/// Wraps an error produced by a helper (which has no access to
/// `origin`) with the position of the slice the helper was working
/// on. Idempotent: if the message already carries position info
/// from a deeper layer, it is left alone.
fn annotate_pos(
    err: EngineError,
    origin: &str,
    slice: &str,
) -> EngineError {
    let suffix = pos_suffix(origin, slice);
    if suffix.is_empty() {
        return err;
    }
    match err {
        EngineError::InvalidTemplate(msg)
            if !msg.contains(" at line ") =>
        {
            EngineError::InvalidTemplate(format!("{msg}{suffix}"))
        }
        EngineError::Render(msg) if !msg.contains(" at line ") => {
            EngineError::Render(format!("{msg}{suffix}"))
        }
        other => other,
    }
}

fn parse_filter(spec: &str) -> (String, Vec<String>) {
    let spec = spec.trim();
    let (name, args_str) = match spec.find(':') {
        Some(i) => (&spec[..i], &spec[i + 1..]),
        None => (spec, ""),
    };
    let args = if args_str.is_empty() {
        Vec::new()
    } else {
        parse_filter_args(args_str)
    };
    (name.trim().to_string(), args)
}

/// Splits a filter argument list on `,`, honouring single- and
/// double-quoted spans so a quoted comma is preserved verbatim. Quotes
/// themselves are stripped from the returned argument values.
fn parse_filter_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quote: Option<char> = None;
    for c in s.chars() {
        match (c, in_quote) {
            ('"', None) => in_quote = Some('"'),
            ('\'', None) => in_quote = Some('\''),
            (q, Some(open)) if q == open => in_quote = None,
            (',', None) => {
                out.push(std::mem::take(&mut buf).trim().to_string());
            }
            (c, _) => buf.push(c),
        }
    }
    let last = buf.trim().to_string();
    if !last.is_empty() || !out.is_empty() {
        out.push(last);
    }
    out
}

/// Applies a single named filter to `input`. Filters that need
/// arguments parse them out of `args`; missing arguments fall back to
/// each filter's documented default. Unknown filter names are reported
/// as a `Render` error so authors get a clear pointer.
fn apply_filter(
    name: &str,
    args: &[String],
    input: String,
) -> Result<String, EngineError> {
    match name {
        "uppercase" => Ok(input.to_uppercase()),
        "lowercase" => Ok(input.to_lowercase()),
        "trim" => Ok(input.trim().to_string()),
        "truncate" => {
            // Default 30 chars (Unicode-aware), suffix "..." appended.
            let limit: usize =
                args.first().and_then(|s| s.parse().ok()).unwrap_or(30);
            let suffix = "...";
            let n = input.chars().count();
            if n > limit {
                let head_len =
                    limit.saturating_sub(suffix.chars().count());
                let mut head: String =
                    input.chars().take(head_len).collect();
                head.push_str(suffix);
                Ok(head)
            } else {
                Ok(input)
            }
        }
        // Capitalize: ASCII-flavoured first-letter uppercase, rest as-is.
        "capitalize" => {
            let mut chars = input.chars();
            Ok(match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars)
                    .collect::<String>(),
                None => input,
            })
        }
        // Length: Unicode character count for strings.
        "length" => Ok(input.chars().count().to_string()),
        // Default: returns the first arg when input is empty,
        // otherwise the input unchanged. `{{ name | default:"anon" }}`.
        "default" => {
            if input.is_empty() {
                Ok(args.first().cloned().unwrap_or_default())
            } else {
                Ok(input)
            }
        }
        // Replace all occurrences of arg 0 with arg 1.
        "replace" => match (args.first(), args.get(1)) {
            (Some(from), Some(to)) => Ok(input.replace(from, to)),
            _ => Err(EngineError::Render(
                "replace filter requires two args: replace:\"from\",\"to\""
                    .to_string(),
            )),
        },
        // URL-encode (RFC 3986 unreserved set is preserved). Hand-rolled
        // to avoid a `urlencoding` dep; correct for query-string use.
        "urlencode" => Ok(url_encode(&input)),
        // Mark a value as already safe — emit raw, *without* the engine's
        // HTML escape on top. This is the filter form of the `{{!key}}`
        // raw opt-out; callers rendering pre-escaped HTML use it from a
        // pipeline (`{{ snippet | safe }}`).
        //
        // Implementation note: the actual escape suppression is handled
        // by the dispatch loop checking for a trailing `safe` filter
        // (see `apply_filter_chain`). Here `safe` is a no-op pass-through
        // so the chain composes naturally.
        "safe" => Ok(input),
        // Numeric filters. Inputs are parsed as f64 so they accept
        // both i64-shaped strings ("42") and float-shaped strings
        // ("3.14"); pass-through identity for ceil/floor/round on
        // values that happen to have no fractional part.
        "abs" => parse_number_filter(&input, "abs", |n| n.abs()),
        "round" => parse_number_filter(&input, "round", |n| n.round()),
        "ceil" => parse_number_filter(&input, "ceil", |n| n.ceil()),
        "floor" => parse_number_filter(&input, "floor", |n| n.floor()),
        "number_format" => {
            let sep =
                args.first().map(String::as_str).unwrap_or(",");
            number_format(&input, sep)
        }
        // String filters. Complement the case-folding / trimming
        // set with shape-changing transforms.
        "repeat" => {
            let n: usize = args
                .first()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    EngineError::Render(
                        "repeat filter requires a positive integer arg"
                            .to_string(),
                    )
                })?;
            Ok(input.repeat(n))
        }
        "reverse" => Ok(input.chars().rev().collect()),
        "slice" => {
            // slice:start,end — Unicode-aware character slicing.
            let start: usize = args
                .first()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    EngineError::Render(
                        "slice filter requires start as the first arg"
                            .to_string(),
                    )
                })?;
            let end: Option<usize> =
                args.get(1).and_then(|s| s.parse().ok());
            let chars: Vec<char> = input.chars().collect();
            let from = start.min(chars.len());
            let to = end.unwrap_or(chars.len()).min(chars.len());
            Ok(chars[from..to.max(from)].iter().collect())
        }
        "pad_start" => pad(&input, args, true),
        "pad_end" => pad(&input, args, false),
        // Boolean-shaped filters: emit "true" or "false". Most
        // useful for direct substitution (e.g. emitting a CSS
        // class conditionally) — for `#if EXPR` you want the
        // built-in `is X` tests or a registered custom test.
        "contains" => Ok(args
            .first()
            .map(|n| input.contains(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        "starts_with" => Ok(args
            .first()
            .map(|n| input.starts_with(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        "ends_with" => Ok(args
            .first()
            .map(|n| input.ends_with(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        unknown => Err(EngineError::Render(format!(
            "Unknown filter: {unknown}"
        ))),
    }
}

/// Pads `s` with `pad_char` (default `' '`) up to `width`
/// characters. `left = true` pads on the left (right-align);
/// `false` pads on the right (left-align). Already-long inputs
/// are returned unchanged.
/// Serialises a `Value` as JSON via `serde_json`. Used by the
/// `json` filter (feature-gated). Required because the standard
/// filter pipeline operates on `String`, which would lose any
/// `List` / `Map` structure (those Display as the empty string).
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
fn json_encode_value(
    v: &crate::context::Value,
) -> Result<String, EngineError> {
    let json = value_to_serde_json(v);
    serde_json::to_string(&json)
        .map_err(|e| EngineError::Render(format!("json filter: {e}")))
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
fn value_to_serde_json(v: &crate::context::Value) -> serde_json::Value {
    use crate::context::Value;
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => {
            serde_json::Value::Number(serde_json::Number::from(*n))
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(
            items.iter().map(value_to_serde_json).collect(),
        ),
        Value::Map(map) => {
            let mut obj = serde_json::Map::new();
            // Sort keys for deterministic output — matches the
            // ordering the engine uses everywhere else (Map iter
            // is unspecified for FnvHashMap).
            let mut keyed: Vec<_> = map.iter().collect();
            keyed.sort_by(|a, b| a.0.cmp(b.0));
            for (k, val) in keyed {
                let _ = obj.insert(k.clone(), value_to_serde_json(val));
            }
            serde_json::Value::Object(obj)
        }
    }
}

fn pad(
    s: &str,
    args: &[String],
    left: bool,
) -> Result<String, EngineError> {
    let width: usize =
        args.first().and_then(|s| s.parse().ok()).ok_or_else(|| {
            EngineError::Render(
                "pad filter requires width as the first arg"
                    .to_string(),
            )
        })?;
    let pad_char =
        args.get(1).and_then(|c| c.chars().next()).unwrap_or(' ');
    let len = s.chars().count();
    if len >= width {
        return Ok(s.to_string());
    }
    let needed = width - len;
    let pad: String =
        std::iter::repeat(pad_char).take(needed).collect();
    Ok(if left {
        format!("{pad}{s}")
    } else {
        format!("{s}{pad}")
    })
}

/// Parses `input` as `f64`, applies `op`, formats the result. If
/// the result has no fractional part it renders as an integer
/// ("3" not "3.0"). Used by abs/round/ceil/floor.
fn parse_number_filter(
    input: &str,
    name: &str,
    op: impl FnOnce(f64) -> f64,
) -> Result<String, EngineError> {
    let n: f64 = input.trim().parse().map_err(|_| {
        EngineError::Render(format!(
            "{name} filter: expected a number, got `{input}`"
        ))
    })?;
    let r = op(n);
    Ok(if r.is_finite() && r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    })
}

/// Inserts `sep` between every group of three digits, counting from
/// the decimal point (or the end of the integer). Preserves a
/// leading `-`. Non-numeric input returns an error.
fn number_format(
    input: &str,
    sep: &str,
) -> Result<String, EngineError> {
    let trimmed = input.trim();
    let (sign, rest) = match trimmed.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", trimmed),
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    if int_part.is_empty()
        || !int_part.chars().all(|c| c.is_ascii_digit())
        || frac_part
            .map_or(false, |f| !f.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(EngineError::Render(format!(
            "number_format filter: expected a number, got `{input}`"
        )));
    }
    let bytes = int_part.as_bytes();
    let mut grouped = String::with_capacity(int_part.len() * 2);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push_str(sep);
        }
        grouped.push(b as char);
    }
    Ok(match frac_part {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    })
}

/// RFC 3986 percent-encoding for the unreserved set (`A-Z a-z 0-9 - _ . ~`).
/// Everything else becomes `%HH`. Hand-rolled to avoid an extra dep.
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => out.push(byte as char),
            other => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

// ─── Expression module ─────────────────────────────────────────────
//
// Tiny recursive-descent grammar used by `{{#if EXPR}}` (and, in later
// phases, by other tag types). C1 added comparison operators; C2 layered
// on boolean operators; C3 layered on integer math; C4 adds the postfix
// `is <test>` predicates (`defined`, `empty`, `none`) with `is not` for
// negation:
//
//   expr       := bool_or
//   bool_or    := bool_and ( "or" bool_and )*
//   bool_and   := bool_not ( "and" bool_not )*
//   bool_not   := "not" bool_not | test_expr
//   test_expr  := comparison ( "is" "not"? TEST_NAME )?
//   comparison := concat_expr ( ("==" | "!=" | "<" | "<=" | ">" | ">=") concat_expr )?
//   concat_expr:= add_expr ( "~" add_expr )*
//   add_expr   := mul_expr ( ("+" | "-") mul_expr )*
//   mul_expr   := operand ( ("*" | "/") operand )*
//   operand    := path | literal
//   literal    := STRING | NUMBER | "true" | "false" | "null"
//   path       := IDENT ("." IDENT)*
//   TEST_NAME  := "defined" | "empty" | "none"
//
// A bare path (`{{#if user}}`) parses as a comparison with no operator
// and evaluates to the path's value, so existing `#if X` callers keep
// working without changes — `is_truthy` runs over the resulting Value.
//
// `and` / `or` / `not` / `is` are reserved when they appear as standalone
// identifiers; dotted paths (e.g. `user.notes`) are unaffected because
// the tokenizer only matches the keyword as a complete identifier. The
// test names (`defined`, `empty`, `none`) are NOT keywords — they parse
// as identifiers and the parser inspects them only after seeing `is`.
//
// Math is integer-only (the only numeric variant `Value` carries is
// `i64`). Mixed-type math (`5 + "x"`) and division by zero return
// `InvalidTemplate` errors so authors get a clear message instead of
// a panic or a silent NaN.

#[derive(Debug, Clone, PartialEq)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
enum Expr {
    Path(String),
    Literal(crate::context::Value),
    Compare(Box<Expr>, CmpOp, Box<Expr>),
    Math(Box<Expr>, MathOp, Box<Expr>),
    /// `lhs ~ rhs` — string concatenation. Both sides are coerced
    /// to their `Display` form (Number → "5", Bool → "true",
    /// Null → "", List/Map → "") and concatenated.
    Concat(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// `lhs is [not] <name>` — postfix predicate with optional
    /// negation. The name is resolved at eval time against
    /// `Engine::custom_tests` first, then the built-in set
    /// (`defined`, `empty`, `none`). Deferring resolution lets a
    /// custom test override a built-in of the same name.
    Test(Box<Expr>, String, bool),
}

impl Expr {
    /// Evaluates the expression against `ctx`, returning the result as a
    /// `Value`. Comparison expressions return `Value::Bool`; bare path
    /// expressions return whatever the lookup resolves to (or `Null`
    /// when missing). The caller decides what to do with the result —
    /// `#if` checks `is_truthy`.
    fn eval(
        &self,
        ctx: &Context,
        engine: &Engine,
    ) -> Result<crate::context::Value, EngineError> {
        use crate::context::Value;
        Ok(match self {
            Expr::Path(p) => {
                ctx.get_path(p).cloned().unwrap_or(Value::Null)
            }
            Expr::Literal(v) => v.clone(),
            Expr::Compare(lhs, op, rhs) => {
                let l = lhs.eval(ctx, engine)?;
                let r = rhs.eval(ctx, engine)?;
                Value::Bool(apply_cmp(op, &l, &r)?)
            }
            Expr::Math(lhs, op, rhs) => {
                let l = lhs.eval(ctx, engine)?;
                let r = rhs.eval(ctx, engine)?;
                Value::Number(apply_math(op, &l, &r)?)
            }
            // String concat. Both sides go through Display, so any
            // primitive (Number, Bool, String) renders sensibly;
            // Null renders as "" and List/Map render as "" too,
            // matching the substitution semantics.
            Expr::Concat(lhs, rhs) => {
                let l = lhs.eval(ctx, engine)?;
                let r = rhs.eval(ctx, engine)?;
                Value::String(format!("{l}{r}"))
            }
            // Boolean operators short-circuit: avoid evaluating the
            // right operand when the left already decides the result.
            // This keeps templates cheap when one side does an
            // expensive lookup or comparison.
            Expr::And(lhs, rhs) => {
                let l = lhs.eval(ctx, engine)?;
                if l.is_truthy() {
                    Value::Bool(rhs.eval(ctx, engine)?.is_truthy())
                } else {
                    Value::Bool(false)
                }
            }
            Expr::Or(lhs, rhs) => {
                let l = lhs.eval(ctx, engine)?;
                if l.is_truthy() {
                    Value::Bool(true)
                } else {
                    Value::Bool(rhs.eval(ctx, engine)?.is_truthy())
                }
            }
            Expr::Not(inner) => {
                Value::Bool(!inner.eval(ctx, engine)?.is_truthy())
            }
            Expr::Test(operand, name, negated) => {
                // Test dispatch: custom-tests checked first so a
                // user can override a built-in of the same name.
                let result = if let Some(f) =
                    engine.custom_tests.get(name)
                {
                    let value = operand.eval(ctx, engine)?;
                    f(&value, &[])?
                } else {
                    match name.as_str() {
                        "defined" => {
                            // `defined` is special: bare path
                            // operands check for path existence
                            // without defaulting to Null. Other
                            // operand shapes reduce to "not Null".
                            if let Expr::Path(p) = operand.as_ref() {
                                ctx.get_path(p).is_some()
                            } else {
                                !matches!(
                                    operand.eval(ctx, engine)?,
                                    Value::Null
                                )
                            }
                        }
                        "empty" => {
                            is_value_empty(&operand.eval(ctx, engine)?)
                        }
                        "none" => matches!(
                            operand.eval(ctx, engine)?,
                            Value::Null
                        ),
                        unknown => {
                            return Err(EngineError::InvalidTemplate(
                                format!(
                                    "unknown test `{unknown}` — \
                                         expected `defined`, `empty`, \
                                         `none`, or a name registered \
                                         via Engine::add_test"
                                ),
                            ));
                        }
                    }
                };
                Value::Bool(if *negated { !result } else { result })
            }
        })
    }
}

/// Compares two values per `op`. `Eq`/`Ne` use structural equality and
/// work on every variant pair. The ordered comparisons (`Lt`/`Le`/`Gt`
/// /`Ge`) require both operands to be numbers or both to be strings;
/// any other combination returns an `InvalidTemplate` error so authors
/// get a clear message instead of silent type coercion.
fn apply_cmp(
    op: &CmpOp,
    lhs: &crate::context::Value,
    rhs: &crate::context::Value,
) -> Result<bool, EngineError> {
    use crate::context::Value;
    use std::cmp::Ordering;
    match op {
        CmpOp::Eq => Ok(lhs == rhs),
        CmpOp::Ne => Ok(lhs != rhs),
        _ => {
            let ord = match (lhs, rhs) {
                (Value::Number(a), Value::Number(b)) => a.cmp(b),
                (Value::String(a), Value::String(b)) => a.cmp(b),
                _ => {
                    return Err(EngineError::InvalidTemplate(format!(
                        "cannot order {lhs:?} and {rhs:?} — \
                         both operands must be numbers or both strings"
                    )));
                }
            };
            Ok(matches!(
                (op, ord),
                (CmpOp::Lt, Ordering::Less)
                    | (CmpOp::Le, Ordering::Less | Ordering::Equal)
                    | (CmpOp::Gt, Ordering::Greater)
                    | (CmpOp::Ge, Ordering::Greater | Ordering::Equal)
            ))
        }
    }
}

/// Applies an integer math op. Both operands must be `Value::Number`;
/// any other combination errors out so authors don't get silent
/// coercion. Division by zero returns an `InvalidTemplate` error
/// rather than panicking.
fn apply_math(
    op: &MathOp,
    lhs: &crate::context::Value,
    rhs: &crate::context::Value,
) -> Result<i64, EngineError> {
    use crate::context::Value;
    let (a, b) = match (lhs, rhs) {
        (Value::Number(a), Value::Number(b)) => (*a, *b),
        _ => {
            return Err(EngineError::InvalidTemplate(format!(
                "math operator requires two numbers, got \
                 {lhs:?} and {rhs:?}"
            )));
        }
    };
    match op {
        MathOp::Add => a.checked_add(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} + {b}"
            ))
        }),
        MathOp::Sub => a.checked_sub(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} - {b}"
            ))
        }),
        MathOp::Mul => a.checked_mul(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} * {b}"
            ))
        }),
        MathOp::Div => {
            if b == 0 {
                Err(EngineError::InvalidTemplate(
                    "division by zero".to_string(),
                ))
            } else {
                a.checked_div(b).ok_or_else(|| {
                    EngineError::InvalidTemplate(format!(
                        "integer overflow in {a} / {b}"
                    ))
                })
            }
        }
    }
}

/// Whether a value is "empty" for `is empty`. Strings, lists, maps
/// each have a natural empty form; `Null` is considered empty so
/// `unset is empty` is true. Numbers and bools are never empty —
/// `0 is empty` is false on purpose, matching Tera/Jinja semantics.
fn is_value_empty(v: &crate::context::Value) -> bool {
    use crate::context::Value;
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::List(l) => l.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Tokenizer + parser entry point. Walks `s` once; whitespace
/// separates tokens but is otherwise insignificant.
fn parse_expr(s: &str) -> Result<Expr, EngineError> {
    let mut tokens = ExprTokens::new(s);
    let expr = parse_bool_or(&mut tokens)?;
    if let Some(extra) = tokens.peek() {
        return Err(EngineError::InvalidTemplate(format!(
            "unexpected token in expression: {extra:?}"
        )));
    }
    Ok(expr)
}

fn parse_bool_or(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_bool_and(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::Or)) {
        let _ = tokens.next();
        let rhs = parse_bool_and(tokens)?;
        lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_bool_and(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_bool_not(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::And)) {
        let _ = tokens.next();
        let rhs = parse_bool_not(tokens)?;
        lhs = Expr::And(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_bool_not(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    if matches!(tokens.peek(), Some(ExprTok::Not)) {
        let _ = tokens.next();
        let inner = parse_bool_not(tokens)?;
        Ok(Expr::Not(Box::new(inner)))
    } else {
        parse_test_expr(tokens)
    }
}

fn parse_test_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let lhs = parse_comparison(tokens)?;
    if !matches!(tokens.peek(), Some(ExprTok::Is)) {
        return Ok(lhs);
    }
    let _ = tokens.next(); // consume `is`
    let negated = matches!(tokens.peek(), Some(ExprTok::Not));
    if negated {
        let _ = tokens.next(); // consume `not`
    }
    let name = match tokens.next() {
        // All names defer to eval-time so a custom test can
        // override a built-in of the same name (matches the
        // add_filter behaviour). Built-in dispatch lives in eval.
        Some(ExprTok::Path(name)) => name,
        Some(other) => {
            return Err(EngineError::InvalidTemplate(format!(
                "expected test name after `is`, got {other:?}"
            )));
        }
        None => {
            return Err(EngineError::InvalidTemplate(
                "expected test name after `is`".to_string(),
            ));
        }
    };
    Ok(Expr::Test(Box::new(lhs), name, negated))
}

fn parse_comparison(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let lhs = parse_concat_expr(tokens)?;
    let op = match tokens.peek() {
        Some(ExprTok::Op(op)) => Some(op.clone()),
        _ => None,
    };
    if let Some(op) = op {
        let _ = tokens.next();
        let rhs = parse_concat_expr(tokens)?;
        Ok(Expr::Compare(Box::new(lhs), op, Box::new(rhs)))
    } else {
        Ok(lhs)
    }
}

fn parse_concat_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_add_expr(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::Tilde)) {
        let _ = tokens.next();
        let rhs = parse_add_expr(tokens)?;
        lhs = Expr::Concat(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_add_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_mul_expr(tokens)?;
    loop {
        let op = match tokens.peek() {
            Some(ExprTok::Plus) => MathOp::Add,
            Some(ExprTok::Minus) => MathOp::Sub,
            _ => break,
        };
        let _ = tokens.next();
        let rhs = parse_mul_expr(tokens)?;
        lhs = Expr::Math(Box::new(lhs), op, Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_mul_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_operand(tokens)?;
    loop {
        let op = match tokens.peek() {
            Some(ExprTok::Star) => MathOp::Mul,
            Some(ExprTok::Slash) => MathOp::Div,
            _ => break,
        };
        let _ = tokens.next();
        let rhs = parse_operand(tokens)?;
        lhs = Expr::Math(Box::new(lhs), op, Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_operand(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    use crate::context::Value;
    match tokens.next() {
        Some(ExprTok::Path(p)) => Ok(Expr::Path(p)),
        Some(ExprTok::Number(n)) => Ok(Expr::Literal(Value::Number(n))),
        Some(ExprTok::String(s)) => Ok(Expr::Literal(Value::String(s))),
        Some(ExprTok::True) => Ok(Expr::Literal(Value::Bool(true))),
        Some(ExprTok::False) => Ok(Expr::Literal(Value::Bool(false))),
        Some(ExprTok::Null) => Ok(Expr::Literal(Value::Null)),
        Some(other) => Err(EngineError::InvalidTemplate(format!(
            "expected operand, got {other:?}"
        ))),
        None => Err(EngineError::InvalidTemplate(
            "expected operand, got end of expression".to_string(),
        )),
    }
}

#[derive(Debug, Clone)]
enum ExprTok {
    Path(String),
    Number(i64),
    String(String),
    True,
    False,
    Null,
    Op(CmpOp),
    And,
    Or,
    Not,
    Is,
    Plus,
    Minus,
    Star,
    Slash,
    /// `~` — string concat (Tera/Twig-style).
    Tilde,
}

/// Single-pass tokenizer. Tokens are produced lazily via `next` /
/// `peek`; we cache one token of lookahead so the parser stays
/// straight-line. Errors surface as `InvalidTemplate` immediately on
/// the offending byte rather than waiting until parse-time.
///
/// `prev_was_operand` disambiguates `-`: when the previous emitted
/// token was an operand (path, literal, closing of a value), `-` is a
/// binary `Minus` operator; otherwise it starts a negative number
/// literal. This lets `5 - 3` and `-3` both parse correctly.
struct ExprTokens<'a> {
    bytes: &'a [u8],
    pos: usize,
    peeked: Option<ExprTok>,
    prev_was_operand: bool,
}

impl<'a> ExprTokens<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
            peeked: None,
            prev_was_operand: false,
        }
    }

    fn peek(&mut self) -> Option<&ExprTok> {
        if self.peeked.is_none() {
            // Capture the current operand-state so the lookahead
            // reflects the parser's true position. peek does not
            // commit, so we must restore on next() emit.
            let saved = self.prev_was_operand;
            self.peeked = self.scan_one();
            self.prev_was_operand = saved;
        }
        self.peeked.as_ref()
    }

    fn next(&mut self) -> Option<ExprTok> {
        let tok = if let Some(tok) = self.peeked.take() {
            Some(tok)
        } else {
            self.scan_one()
        };
        if let Some(t) = &tok {
            self.prev_was_operand = is_operand_tok(t);
        }
        tok
    }

    fn scan_one(&mut self) -> Option<ExprTok> {
        // Skip whitespace.
        while self.pos < self.bytes.len()
            && self.bytes[self.pos].is_ascii_whitespace()
        {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        // Two-character comparison operators come first so they win
        // over the single-char prefix check below.
        if self.pos + 1 < self.bytes.len() {
            let two = &self.bytes[self.pos..self.pos + 2];
            let op = match two {
                b"==" => Some(CmpOp::Eq),
                b"!=" => Some(CmpOp::Ne),
                b"<=" => Some(CmpOp::Le),
                b">=" => Some(CmpOp::Ge),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 2;
                return Some(ExprTok::Op(op));
            }
        }
        // Single-char comparison operators.
        match b {
            b'<' => {
                self.pos += 1;
                return Some(ExprTok::Op(CmpOp::Lt));
            }
            b'>' => {
                self.pos += 1;
                return Some(ExprTok::Op(CmpOp::Gt));
            }
            _ => {}
        }
        // Math operators. `-` is binary only when an operand just
        // closed; otherwise it's the sign on a numeric literal.
        match b {
            b'+' => {
                self.pos += 1;
                return Some(ExprTok::Plus);
            }
            b'*' => {
                self.pos += 1;
                return Some(ExprTok::Star);
            }
            b'/' => {
                self.pos += 1;
                return Some(ExprTok::Slash);
            }
            b'-' if self.prev_was_operand => {
                self.pos += 1;
                return Some(ExprTok::Minus);
            }
            // `~` is the string-concat operator. Always binary;
            // never overloaded as a unary prefix, so no
            // prev_was_operand check.
            b'~' => {
                self.pos += 1;
                return Some(ExprTok::Tilde);
            }
            _ => {}
        }
        // String literal.
        if b == b'"' || b == b'\'' {
            return self.scan_string(b);
        }
        // Number literal (optionally signed).
        if b == b'-' || b.is_ascii_digit() {
            return self.scan_number();
        }
        // Path / keyword.
        if is_ident_start(b) {
            return self.scan_path_or_keyword();
        }
        // Unknown byte — let the parser flag it via a None / consume
        // loop. Advance so we don't spin forever.
        self.pos += 1;
        None
    }

    fn scan_string(&mut self, quote: u8) -> Option<ExprTok> {
        self.pos += 1; // consume opening quote
        let start = self.pos;
        while self.pos < self.bytes.len()
            && self.bytes[self.pos] != quote
        {
            self.pos += 1;
        }
        let raw = &self.bytes[start..self.pos];
        // Skip closing quote (or accept end-of-input as unterminated).
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
        Some(ExprTok::String(String::from_utf8_lossy(raw).into_owned()))
    }

    fn scan_number(&mut self) -> Option<ExprTok> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.bytes.len()
            && self.bytes[self.pos].is_ascii_digit()
        {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .parse::<i64>()
            .ok()?;
        Some(ExprTok::Number(raw))
    }

    fn scan_path_or_keyword(&mut self) -> Option<ExprTok> {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && is_ident_continue(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .to_string();
        Some(match raw.as_str() {
            "true" => ExprTok::True,
            "false" => ExprTok::False,
            "null" => ExprTok::Null,
            "and" => ExprTok::And,
            "or" => ExprTok::Or,
            "not" => ExprTok::Not,
            "is" => ExprTok::Is,
            _ => ExprTok::Path(raw),
        })
    }
}

fn is_ident_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'@')
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || matches!(b, b'0'..=b'9' | b'.')
}

/// Whether `tok` is an operand-shaped token. Used by the tokenizer to
/// decide if `-` should start a negative number literal (when the
/// previous token was *not* an operand) or be a binary subtraction
/// (when it was). Keywords like `true`/`false`/`null` count too —
/// they're literals.
fn is_operand_tok(tok: &ExprTok) -> bool {
    matches!(
        tok,
        ExprTok::Path(_)
            | ExprTok::Number(_)
            | ExprTok::String(_)
            | ExprTok::True
            | ExprTok::False
            | ExprTok::Null
    )
}

// ─── End expression module ─────────────────────────────────────────

/// Parses a `name = literal` assignment used by `{{#set}}`. The literal
/// follows the same grammar as a partial parameter value: quoted string,
/// integer, bool, null, or bareword (treated as a literal string).
fn parse_set_assignment(
    s: &str,
) -> Result<(String, crate::context::Value), EngineError> {
    let s = s.trim();
    let eq = s.find('=').ok_or_else(|| {
        EngineError::InvalidTemplate(
            "#set: missing `= value`".to_string(),
        )
    })?;
    let name = s[..eq].trim().to_string();
    if name.is_empty() {
        return Err(EngineError::InvalidTemplate(
            "#set: empty name".to_string(),
        ));
    }
    let value_str = s[eq + 1..].trim();
    if value_str.is_empty() {
        return Err(EngineError::InvalidTemplate(format!(
            "#set `{name}`: missing value"
        )));
    }
    Ok((name, parse_literal_value(value_str)))
}

/// Recognises a literal token: `"quoted"` / `'quoted'` strings,
/// `true`/`false`, `null`, integers, and bareword fallback (string).
fn parse_literal_value(s: &str) -> crate::context::Value {
    let bytes = s.as_bytes();
    if s.len() >= 2 {
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
        {
            return crate::context::Value::String(
                s[1..s.len() - 1].to_string(),
            );
        }
    }
    match s {
        "true" => crate::context::Value::Bool(true),
        "false" => crate::context::Value::Bool(false),
        "null" => crate::context::Value::Null,
        _ => match s.parse::<i64>() {
            Ok(n) => crate::context::Value::Number(n),
            Err(_) => crate::context::Value::String(s.to_string()),
        },
    }
}

/// Strips matching single or double quotes from a name token. Returns
/// the inner content unchanged if the token is unquoted.
fn parse_block_name(s: &str) -> Result<&str, EngineError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(EngineError::InvalidTemplate(
            "missing block name".to_string(),
        ));
    }
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
        {
            return Ok(&s[1..s.len() - 1]);
        }
    }
    Ok(s)
}

/// Returns the base-template name if `template`'s first non-whitespace
/// tag is `{{#extends "name"}}`, otherwise `None`. Quoted or bareword
/// names both work.
fn parse_extends<'a>(
    template: &'a str,
    open: &str,
    close: &str,
) -> Result<Option<&'a str>, EngineError> {
    let trimmed = template.trim_start();
    if !trimmed.starts_with(open) {
        return Ok(None);
    }
    let after_open = &trimmed[open.len()..];
    let end = after_open.find(close).ok_or_else(|| {
        EngineError::InvalidTemplate(
            "Unclosed template tag".to_string(),
        )
    })?;
    let inner = parse_ws_control(after_open[..end].trim()).0;
    if let Some(name_part) = inner.strip_prefix("#extends") {
        Ok(Some(parse_block_name(name_part.trim())?))
    } else {
        Ok(None)
    }
}

/// Walks a child template collecting every top-level
/// `{{#block "name"}}…{{/block}}` declaration into an owned name → body
/// map. Non-block tags (including the leading `#extends`) are skipped;
/// any literal text between blocks is silently dropped, matching the
/// Tera/Jinja convention that child templates only contribute block
/// overrides.
fn collect_blocks(
    template: &str,
    open: &str,
    close: &str,
) -> Result<BlockOverrides, EngineError> {
    let mut out = BlockOverrides::new();
    let mut rest = template;
    while let Some(start) = rest.find(open) {
        let after_open = &rest[start + open.len()..];
        let end = after_open.find(close).ok_or_else(|| {
            EngineError::InvalidTemplate(
                "Unclosed template tag".to_string(),
            )
        })?;
        let inner = parse_ws_control(after_open[..end].trim()).0;
        let after_tag = &after_open[end + close.len()..];

        if let Some(name_part) = inner.strip_prefix("#block") {
            let name = parse_block_name(name_part.trim())?;
            let (body, after_block) =
                extract_block(after_tag, "block", open, close)?;
            let _ = out.insert(name.to_string(), body.to_string());
            rest = after_block;
        } else {
            // Not a block opener (e.g. `#extends`, comments, literal
            // tags) — skip past it and keep scanning.
            rest = after_tag;
        }
    }
    Ok(out)
}

/// Splits a partial invocation at the first whitespace, separating the
/// partial name from its `k=v` parameter list. `name` is everything up
/// to the first space; `params` is everything after, trimmed.
///
///   "footer"            -> ("footer", "")
///   "footer year=2026"  -> ("footer", "year=2026")
fn split_partial_invocation(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    }
}

/// Parses a `k=v k2="v 2" k3=42 k4=true` parameter list into a vector of
/// `(name, Value)` pairs. Values may be:
///
///   - quoted strings: `"…"` or `'…'` (preserve embedded spaces)
///   - integer literals: `42`, `-7`
///   - booleans: `true`, `false`
///   - null: `null`
///   - bare identifiers: treated as literal strings
///
/// Whitespace separates pairs; unbalanced quotes return an
/// `InvalidTemplate` error.
fn parse_partial_params(
    s: &str,
) -> Result<Vec<(String, crate::context::Value)>, EngineError> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            let _ = chars.next();
            continue;
        }
        // Read key up to '='
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            let _ = chars.next();
        }
        if key.is_empty() {
            return Err(EngineError::InvalidTemplate(
                "partial param: empty key".to_string(),
            ));
        }
        if chars.next() != Some('=') {
            return Err(EngineError::InvalidTemplate(format!(
                "partial param `{key}` missing `=value`"
            )));
        }
        // Read value: quoted run, or whitespace-terminated bareword.
        let mut value = String::new();
        match chars.peek() {
            Some(&q @ ('"' | '\'')) => {
                let _ = chars.next();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == q {
                        closed = true;
                        break;
                    }
                    value.push(c);
                }
                if !closed {
                    return Err(EngineError::InvalidTemplate(format!(
                        "partial param `{key}` has unclosed quote"
                    )));
                }
                out.push((key, crate::context::Value::String(value)));
            }
            Some(_) => {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    let _ = chars.next();
                }
                let parsed = match value.as_str() {
                    "true" => crate::context::Value::Bool(true),
                    "false" => crate::context::Value::Bool(false),
                    "null" => crate::context::Value::Null,
                    _ => match value.parse::<i64>() {
                        Ok(n) => crate::context::Value::Number(n),
                        Err(_) => crate::context::Value::String(value),
                    },
                };
                out.push((key, parsed));
            }
            None => {
                return Err(EngineError::InvalidTemplate(format!(
                    "partial param `{key}` missing value"
                )));
            }
        }
    }
    Ok(out)
}

/// Parses whitespace-control dashes off a (whitespace-trimmed) tag inner.
/// Returns the inner with any dashes removed, plus two flags reporting
/// whether a left and/or right dash was present. Block comments
/// (`!-- … --`) are exempt — the closing `--` is part of their syntax.
fn parse_ws_control(inner: &str) -> (&str, bool, bool) {
    if inner.starts_with("!--") {
        return (inner, false, false);
    }
    let (inner, strip_l) = match inner.strip_prefix('-') {
        Some(rest) => (rest.trim_start(), true),
        None => (inner, false),
    };
    let (inner, strip_r) = match inner.strip_suffix('-') {
        Some(rest) => (rest.trim_end(), true),
        None => (inner, false),
    };
    (inner, strip_l, strip_r)
}

/// Convenience: strip whitespace-control dashes and discard the flags
/// (used by helpers that only need the cleaned inner string for matching).
fn strip_ws_dashes(inner: &str) -> &str {
    parse_ws_control(inner).0
}

/// Splits a `#if` body at the top-level `{{else}}`, if any. Returns
/// `(then_body, else_body)`. Nested blocks are skipped via the same depth
/// counter used by [`extract_block`].
fn split_else<'a>(
    body: &'a str,
    open: &str,
    close: &str,
) -> (&'a str, Option<&'a str>) {
    let mut depth: usize = 0;
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find(open) {
        let abs = cursor + rel;
        let after_open = &body[abs + open.len()..];
        let Some(end) = after_open.find(close) else {
            // Malformed — let the recursive render call surface the error.
            break;
        };
        let inner = strip_ws_dashes(after_open[..end].trim());
        let tag_end = abs + open.len() + end + close.len();

        if inner.starts_with("#if")
            || inner.starts_with("#each")
            || inner.starts_with("#block")
        {
            depth += 1;
        } else if inner.starts_with("/if")
            || inner.starts_with("/each")
            || inner.starts_with("/block")
        {
            depth = depth.saturating_sub(1);
        } else if inner == "else" && depth == 0 {
            return (&body[..abs], Some(&body[tag_end..]));
        }
        cursor = tag_end;
    }
    (body, None)
}

/// Appends `s` to `out`, replacing the five HTML metacharacters with their
/// named/numeric entities. Single-quote uses the numeric `&#x27;` form so
/// the output stays valid inside both HTML and XML attributes.
///
/// Byte-level scan: iterate over `s.as_bytes()`, flush clean runs via
/// Append `s` to `out` with `& < > " '` substituted for their HTML
/// entities. Same five-character set as Askama's `Html` escaper.
/// Delegates to `askama_escape`, which auto-vectorises the inner loop
/// with SIMD (SSE4.2 / AVX2 / NEON depending on target) for a ~3-10×
/// speedup on long strings vs the scalar byte scan we used previously.
fn escape_html_into(s: &str, out: &mut String) {
    use std::fmt::Write as _;
    out.reserve(s.len());
    // `write!` against `String` is infallible; the result is discarded
    // explicitly to satisfy `unused_results`.
    let _ = write!(
        out,
        "{}",
        askama_escape::escape(s, askama_escape::Html)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Context;

    #[test]
    fn test_render_template() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("<<", ">>");
        let mut context = Context::new();
        context.set("name".to_string(), "Alice".to_string());
        context.set("greeting".to_string(), "Hello".to_string());

        let template = "<<greeting>>, <<name>>!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_render_template_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let context = Context::new();

        let template = "";
        let result = engine.render_template(template, &context);
        assert!(matches!(result, Err(EngineError::InvalidTemplate(_))));
    }

    #[test]
    fn test_render_template_bare_open_char_is_literal() {
        // A single `{` with no matching `{{` is literal text, not an error.
        // Previously rejected by a broken heuristic.
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("{{", "}}");
        let context = Context::new();
        let template = "Hello, {name}!";
        let result = engine.render_template(template, &context);
        assert_eq!(result.unwrap(), "Hello, {name}!");
    }

    #[test]
    fn test_render_template_nested_delimiters_rejected() {
        let engine = Engine::new("", Duration::from_secs(60));
        let context = Context::new();
        let template = "{{outer{{inner}}}}";
        let result = engine.render_template(template, &context);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("Nested")
        ));
    }

    #[test]
    fn test_render_template_custom_delimiters() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("<<", ">>");
        let mut context = Context::new();
        context.set("name".to_string(), "Alice".to_string());
        context.set("greeting".to_string(), "Hello".to_string());

        let template = "<<greeting>>, <<name>>!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice!");

        // Bare `<` is literal text under the new parser.
        let literal_template = "Hello, <name>!";
        let result =
            engine.render_template(literal_template, &context).unwrap();
        assert_eq!(result, "Hello, <name>!");
    }

    #[test]
    fn test_render_template_escapes_html_by_default() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set(
            "name".to_string(),
            "<script>alert('x')</script>".to_string(),
        );
        let result =
            engine.render_template("Hi {{name}}", &context).unwrap();
        assert_eq!(
            result,
            "Hi &lt;script&gt;alert(&#x27;x&#x27;)&lt;/script&gt;"
        );
    }

    #[test]
    fn render_template_resolves_dot_notation() {
        use crate::context::Value;
        use fnv::FnvHashMap;

        let engine = Engine::new("", Duration::from_secs(60));
        let mut user = FnvHashMap::default();
        let _ = user.insert(
            "name".to_string(),
            Value::String("Ada".to_string()),
        );
        let mut ctx = Context::new();
        ctx.set_value("user".to_string(), Value::Map(user));

        let out = engine
            .render_template("Hello, {{user.name}}!", &ctx)
            .unwrap();
        assert_eq!(out, "Hello, Ada!");
    }

    #[test]
    fn render_template_renders_primitive_values() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("count".to_string(), 42);
        ctx.set_value("active".to_string(), true);

        let out = engine
            .render_template("count={{count}} active={{active}}", &ctx)
            .unwrap();
        assert_eq!(out, "count=42 active=true");
    }

    #[test]
    fn if_block_renders_when_truthy() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("show".to_string(), true);
        let out = engine
            .render_template("{{#if show}}hello{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "hello");
    }

    #[test]
    fn if_block_skips_when_falsy() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("show".to_string(), false);
        let out = engine
            .render_template("[{{#if show}}hi{{/if}}]", &ctx)
            .unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn if_block_with_else_branch() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("on".to_string(), false);
        let out = engine
            .render_template("{{#if on}}yes{{else}}no{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "no");
    }

    #[test]
    fn each_block_iterates_list_with_this_binding() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
        let out = engine
            .render_template("{{#each items}}[{{this}}]{{/each}}", &ctx)
            .unwrap();
        assert_eq!(out, "[a][b][c]");
    }

    #[test]
    fn nested_if_inside_each_resolves() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["x", "y"]);
        ctx.set_value("show".to_string(), true);
        let out = engine
            .render_template(
                "{{#each items}}{{#if show}}{{this}}{{/if}}{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "xy");
    }

    #[test]
    fn unclosed_if_block_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("on".to_string(), true);
        let res = engine.render_template("{{#if on}}forever", &ctx);
        assert!(matches!(
            res,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("#if")
        ));
    }

    #[test]
    fn stray_close_tag_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let res = engine.render_template("nope {{/if}}", &ctx);
        assert!(matches!(
            res,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("/if")
        ));
    }

    #[test]
    fn each_on_non_list_errors() {
        use crate::context::Value;
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("not_a_list".to_string(), Value::Number(1));
        let res = engine
            .render_template("{{#each not_a_list}}x{{/each}}", &ctx);
        assert!(matches!(res, Err(EngineError::InvalidTemplate(_))));
    }

    #[test]
    fn render_template_indexes_lists_by_position() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);

        let out = engine
            .render_template("first={{items.0}} last={{items.2}}", &ctx)
            .unwrap();
        assert_eq!(out, "first=a last=c");
    }

    #[test]
    fn test_render_template_raw_opt_out() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set("body".to_string(), "<b>hi</b>".to_string());
        let result =
            engine.render_template("{{!body}}", &context).unwrap();
        assert_eq!(result, "<b>hi</b>");
    }

    #[test]
    fn comment_inline_renders_nothing() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out =
            engine.render_template("[{{! ignored }}]", &ctx).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn comment_block_form_renders_nothing() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template("[{{!-- block\nspans lines --}}]", &ctx)
            .unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn comment_empty_form_renders_nothing() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine.render_template("[{{!}}]", &ctx).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn whitespace_strip_left_removes_trailing_output_spaces() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out =
            engine.render_template("hello   {{- name}}", &ctx).unwrap();
        assert_eq!(out, "helloAda");
    }

    #[test]
    fn whitespace_strip_right_skips_leading_chunk_spaces() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out =
            engine.render_template("{{name -}}   tail", &ctx).unwrap();
        assert_eq!(out, "Adatail");
    }

    #[test]
    fn whitespace_strip_both_collapses_around_tag() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine
            .render_template("a   {{- name -}}\n\n   b", &ctx)
            .unwrap();
        assert_eq!(out, "aAdab");
    }

    #[test]
    fn whitespace_strip_composes_with_inline_comment() {
        // `{{- ! note -}}` is "strip left + inline comment + strip right"
        // — useful as a pure whitespace-eating marker.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template("a   {{- ! note -}}\n\n   b", &ctx)
            .unwrap();
        assert_eq!(out, "ab");
    }

    #[test]
    fn filter_truncate_accepts_explicit_limit() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("body".to_string(), "hello world".to_string());
        let out = engine
            .render_template("{{ body | truncate:8 }}", &ctx)
            .unwrap();
        assert_eq!(out, "hello...");
    }

    #[test]
    fn filter_truncate_falls_back_to_default_30() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("body".to_string(), "x".repeat(50).to_string());
        let out = engine
            .render_template("{{ body | truncate }}", &ctx)
            .unwrap();
        assert_eq!(out.chars().count(), 30);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn parse_filter_recognises_quoted_commas() {
        let (name, args) = parse_filter(r#"replace:"a, b","c""#);
        assert_eq!(name, "replace");
        assert_eq!(args, vec!["a, b".to_string(), "c".to_string()]);
    }

    #[test]
    fn parse_filter_handles_no_args() {
        let (name, args) = parse_filter("uppercase");
        assert_eq!(name, "uppercase");
        assert!(args.is_empty());
    }

    #[test]
    fn filter_capitalize_uppercases_first_char() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "alice".to_string());
        let out = engine
            .render_template("{{ name | capitalize }}", &ctx)
            .unwrap();
        assert_eq!(out, "Alice");
    }

    #[test]
    fn filter_length_counts_chars() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Adriana".to_string());
        let out = engine
            .render_template("{{ name | length }}", &ctx)
            .unwrap();
        assert_eq!(out, "7");
    }

    #[test]
    fn filter_default_substitutes_for_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), String::new());
        let out = engine
            .render_template(r#"{{ name | default:"anon" }}"#, &ctx)
            .unwrap();
        assert_eq!(out, "anon");
    }

    #[test]
    fn filter_default_passes_through_non_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine
            .render_template(r#"{{ name | default:"anon" }}"#, &ctx)
            .unwrap();
        assert_eq!(out, "Ada");
    }

    #[test]
    fn filter_replace_substitutes_substring() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("msg".to_string(), "hello world".to_string());
        let out = engine
            .render_template(
                r#"{{ msg | replace:"world","there" }}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "hello there");
    }

    #[test]
    fn filter_replace_errors_without_two_args() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("msg".to_string(), "x".to_string());
        let err = engine
            .render_template(r#"{{ msg | replace:"only" }}"#, &ctx)
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::Render(msg) if msg.contains("two args"),
        ));
    }

    #[test]
    fn filter_urlencode_handles_special_chars() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("q".to_string(), "rust lang & you".to_string());
        let out = engine
            .render_template("{{ q | urlencode }}", &ctx)
            .unwrap();
        assert_eq!(out, "rust%20lang%20%26%20you");
    }

    #[test]
    fn filter_safe_suppresses_html_escape() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("body".to_string(), "<b>hi</b>".to_string());
        // Without `safe`, default escape applies.
        let escaped =
            engine.render_template("{{ body }}", &ctx).unwrap();
        assert_eq!(escaped, "&lt;b&gt;hi&lt;/b&gt;");
        // With trailing `safe`, raw output.
        let raw =
            engine.render_template("{{ body | safe }}", &ctx).unwrap();
        assert_eq!(raw, "<b>hi</b>");
    }

    // ── E1: Custom filters API ─────────────────────────────────────

    #[test]
    fn add_filter_registers_custom_pipeline_step() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_filter(
            "shout",
            Arc::new(|input, _args| {
                Ok(format!("{}!!!", input.to_uppercase()))
            }),
        );
        let mut ctx = Context::new();
        ctx.set("greeting".to_string(), "hello".to_string());
        let out = engine
            .render_template("{{ greeting | shout }}", &ctx)
            .unwrap();
        // shout produces "HELLO!!!"; HTML escape leaves '!' alone.
        assert_eq!(out, "HELLO!!!");
    }

    #[test]
    fn add_filter_receives_arguments() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_filter(
            "wrap",
            Arc::new(|input, args: &[String]| {
                let pre =
                    args.first().map(String::as_str).unwrap_or("[");
                let post =
                    args.get(1).map(String::as_str).unwrap_or("]");
                Ok(format!("{pre}{input}{post}"))
            }),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine
            .render_template(r#"{{ name | wrap:"<<",">>" }}"#, &ctx)
            .unwrap();
        // The output is HTML-escaped after the filter runs, so `<<`
        // becomes `&lt;&lt;`. That's the same contract as built-in
        // filters — `safe` is the documented opt-out.
        assert_eq!(out, "&lt;&lt;Ada&gt;&gt;");
    }

    #[test]
    fn add_filter_overrides_builtin_with_same_name() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        // Override `uppercase` to do the opposite — proves precedence.
        let _ = engine.add_filter(
            "uppercase",
            Arc::new(|input, _args| Ok(input.to_lowercase())),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "ADA".to_string());
        let out = engine
            .render_template("{{ name | uppercase }}", &ctx)
            .unwrap();
        assert_eq!(out, "ada");
    }

    #[test]
    fn add_filter_propagates_errors_from_user_code() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_filter(
            "boom",
            Arc::new(|_input, _args| {
                Err(EngineError::Render("filter exploded".to_string()))
            }),
        );
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let err =
            engine.render_template("{{ x | boom }}", &ctx).unwrap_err();
        assert!(
            format!("{err}").contains("filter exploded"),
            "expected filter error to propagate, got {err}"
        );
    }

    // ── E2: Stream rendering ───────────────────────────────────────

    // ── E3: Line numbers in error messages ─────────────────────────

    #[test]
    fn unresolved_tag_error_reports_line_and_column() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // Line 2: "Hello {{missing}}". The lookup `missing` sits at
        // column 9 (after "Hello " + "{{").
        let template = "Header\nHello {{missing}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("missing"), "got: {msg}");
        assert!(msg.contains("line 2"), "got: {msg}");
        assert!(msg.contains("column 9"), "got: {msg}");
    }

    #[test]
    fn unresolved_each_list_reports_line_and_column() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let template = "\n\n{{#each undefined_list}}{{this}}{{/each}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("undefined_list"), "got: {msg}");
        assert!(msg.contains("line 3"), "got: {msg}");
    }

    #[test]
    fn unclosed_tag_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let template = "Line one\nLine two\nLine three {{ unclosed";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Unclosed"), "got: {msg}");
        assert!(msg.contains("line 3"), "got: {msg}");
    }

    #[test]
    fn stray_closing_tag_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let template = "ok\n{{/if}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("/if"), "got: {msg}");
        assert!(msg.contains("line 2"), "got: {msg}");
    }

    #[test]
    fn line_one_errors_report_line_one() {
        // Single-line template: error should still say `line 1`.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err = engine.render_template("{{nope}}", &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 1"), "got: {msg}");
        assert!(msg.contains("column 3"), "got: {msg}");
    }

    // ── F1: Line:col on the remaining error sites ──────────────────

    #[test]
    fn malformed_if_expression_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // Bad expression: missing operand after `==`
        let template = "ok\n{{#if a == }}x{{/if}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 2"), "got: {msg}");
    }

    #[test]
    fn unclosed_if_block_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // No matching {{/if}} — extract_block fails.
        let template = "header\n{{#if true}}body\nstill body";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 2"), "got: {msg}");
    }

    #[test]
    fn malformed_set_assignment_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // Missing `=` makes parse_set_assignment fail.
        let template = "ok\n{{#set badname}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 2"), "got: {msg}");
    }

    // ── Coverage: hit ≥98% line coverage ───────────────────────────

    #[test]
    fn engine_debug_impl_lists_filter_names() {
        use std::sync::Arc;
        let mut engine = Engine::new("t", Duration::from_secs(60));
        let _ = engine.add_filter(
            "shout",
            Arc::new(|input, _| Ok(input.to_uppercase())),
        );
        let s = format!("{engine:?}");
        assert!(s.contains("Engine"), "{s}");
        assert!(s.contains("shout"), "{s}");
        assert!(s.contains("template_path"), "{s}");
    }

    #[test]
    fn render_page_to_writes_into_a_buffer() {
        let temp = tempfile::TempDir::new().unwrap();
        let layout = temp.path().join("hello.html");
        fs::write(&layout, "Hello, {{name}}!").unwrap();
        let mut engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let mut buf: Vec<u8> = Vec::new();
        engine.render_page_to(&ctx, "hello", &mut buf).unwrap();
        assert_eq!(buf, b"Hello, Ada!");
    }

    #[test]
    fn pos_suffix_empty_for_unrelated_slice() {
        // A slice that isn't a substring of `origin` returns "".
        let origin = "alpha";
        let other = String::from("beta");
        assert_eq!(pos_suffix(origin, &other), "");
    }

    #[test]
    fn annotate_pos_passes_through_io_errors() {
        // Only InvalidTemplate / Render get position-stamped; Io
        // (and other variants) flow through unchanged.
        let err = EngineError::Io(io::Error::new(
            io::ErrorKind::Other,
            "raw",
        ));
        let template = "abc";
        let wrapped = annotate_pos(err, template, &template[..1]);
        match wrapped {
            EngineError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn annotate_pos_is_idempotent() {
        // A message that already carries `at line N, column M` is
        // not stamped twice.
        let err =
            EngineError::Render("boom at line 1, column 2".to_string());
        let template = "abc";
        let wrapped = annotate_pos(err, template, &template[..1]);
        if let EngineError::Render(msg) = wrapped {
            // Substring "at line " appears exactly once.
            let count = msg.matches("at line ").count();
            assert_eq!(count, 1, "double-stamped: {msg}");
        } else {
            panic!("expected Render variant");
        }
    }

    #[test]
    fn filter_truncate_pass_through_when_input_short() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "hi".to_string());
        let out = engine
            .render_template("{{ s | truncate:30 }}", &ctx)
            .unwrap();
        assert_eq!(out, "hi");
    }

    #[test]
    fn filter_capitalize_on_empty_input() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), String::new());
        let out = engine
            .render_template("{{ s | capitalize }}", &ctx)
            .unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn filter_round_keeps_fractional_when_op_returns_non_integer() {
        // op = identity-on-fraction; result keeps the decimal.
        // We don't have a built-in op that returns a fractional
        // float, so exercise the format branch via parse_number_filter
        // directly with a custom op.
        let s = parse_number_filter("3", "abs", |n| n + 0.5).unwrap();
        assert_eq!(s, "3.5");
    }

    #[test]
    fn parse_operand_errors_at_end_of_expression() {
        // Empty expression has no operand at all.
        let err = parse_expr("").unwrap_err();
        assert!(
            format!("{err}").contains("end of expression"),
            "{err}"
        );
    }

    #[test]
    fn parse_operand_errors_on_keyword_in_operand_position() {
        // Starting an expression with `and` is a parse error.
        let err = parse_expr("and 1").unwrap_err();
        assert!(format!("{err}").contains("expected operand"), "{err}");
    }

    #[test]
    fn is_test_errors_on_missing_name() {
        // `x is` with nothing after is a parse error.
        let err = parse_expr("x is").unwrap_err();
        assert!(
            format!("{err}").contains("expected test name"),
            "{err}"
        );
    }

    #[test]
    fn is_test_errors_on_non_path_token_after_is() {
        // `x is 5` — the test name must be one of defined/empty/none,
        // and 5 isn't a Path token at all.
        let err = parse_expr("x is 5").unwrap_err();
        assert!(
            format!("{err}").contains("expected test name"),
            "{err}"
        );
    }

    #[test]
    fn set_assignment_empty_name_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err =
            engine.render_template("{{#set = 1}}", &ctx).unwrap_err();
        assert!(format!("{err}").contains("empty name"), "{err}");
    }

    #[test]
    fn set_assignment_empty_value_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err =
            engine.render_template("{{#set x = }}", &ctx).unwrap_err();
        // Just check the error fires; message comes from parse_set_assignment.
        assert!(
            matches!(err, EngineError::InvalidTemplate(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn partial_params_empty_key_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "x").unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let err =
            engine.render_template("{{> p =1}}", &ctx).unwrap_err();
        assert!(format!("{err}").contains("empty key"), "{err}");
    }

    #[test]
    fn partial_params_missing_equals_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "x").unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let err =
            engine.render_template("{{> p key1}}", &ctx).unwrap_err();
        assert!(format!("{err}").contains("missing"), "{err}");
    }

    #[test]
    fn annotate_pos_stamps_bare_render_message() {
        // A Render error without existing position info gets the
        // suffix appended.
        let err = EngineError::Render("plain message".to_string());
        let template = "abcdef";
        let wrapped = annotate_pos(err, template, &template[3..]);
        if let EngineError::Render(msg) = wrapped {
            assert!(msg.contains("plain message"), "{msg}");
            assert!(msg.contains("at line 1"), "{msg}");
            assert!(msg.contains("column 4"), "{msg}");
        } else {
            panic!("expected Render");
        }
    }

    #[test]
    fn annotate_pos_stamps_bare_invalid_template_message() {
        let err =
            EngineError::InvalidTemplate("syntax oops".to_string());
        let template = "x";
        let wrapped = annotate_pos(err, template, &template[..1]);
        if let EngineError::InvalidTemplate(msg) = wrapped {
            assert!(msg.contains("syntax oops"), "{msg}");
            assert!(msg.contains("at line 1"), "{msg}");
        } else {
            panic!("expected InvalidTemplate");
        }
    }

    #[test]
    fn parse_operand_accepts_true_and_false_literals() {
        // Exercises the True/False arms in parse_operand.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let yes = engine
            .render_template(
                "{{#if true == true}}y{{else}}n{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(yes, "y");
        let no = engine
            .render_template(
                "{{#if false == true}}y{{else}}n{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(no, "n");
    }

    #[test]
    fn parse_block_name_accepts_quoted_names() {
        // Exercises the quoted-strip return in parse_block_name.
        assert_eq!(parse_block_name("\"title\"").unwrap(), "title");
        assert_eq!(parse_block_name("'foo'").unwrap(), "foo");
    }

    #[test]
    fn parse_extends_unclosed_tag_errors() {
        // {{#extends with no closing }} — exercises the unclosed
        // path in parse_extends.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err = engine
            .render_template(r#"{{#extends "base"#, &ctx)
            .unwrap_err();
        assert!(
            matches!(err, EngineError::InvalidTemplate(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn math_overflow_triggers_for_subtraction() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("min".to_string(), i64::MIN);
        let err = engine
            .render_template("{{#if min - 1 == 0}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("integer overflow"), "{err}");
    }

    #[test]
    fn math_overflow_triggers_for_multiplication() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("max".to_string(), i64::MAX);
        let err = engine
            .render_template("{{#if max * 2 == 0}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("integer overflow"), "{err}");
    }

    #[test]
    fn math_overflow_triggers_for_integer_division() {
        // i64::MIN / -1 overflows because the result doesn't fit in i64.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("min".to_string(), i64::MIN);
        let err = engine
            .render_template("{{#if min / -1 == 0}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("integer overflow"), "{err}");
    }

    #[test]
    fn partial_params_quoted_value_with_special_chars() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "got={{label}}").unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine
            .render_template(r#"{{> p label="hi there"}}"#, &ctx)
            .unwrap();
        assert_eq!(out, "got=hi there");
    }

    #[test]
    fn partial_params_unquoted_value_parsed_as_typed_literal() {
        // Exercises the bareword parsing branch (true/false/null/int/string).
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(
            temp.path().join("p.html"),
            "{{#if flag}}on{{else}}off{{/if}}/{{count}}",
        )
        .unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine
            .render_template("{{> p flag=true count=42}}", &ctx)
            .unwrap();
        assert_eq!(out, "on/42");
    }

    #[test]
    fn parse_expr_errors_on_trailing_junk() {
        // Exercises the "unexpected token in expression" path in
        // parse_expr — a complete expression followed by extra
        // tokens.
        let err = parse_expr("1 == 1 trailing").unwrap_err();
        assert!(format!("{err}").contains("unexpected token"), "{err}");
    }

    #[test]
    fn url_encode_passes_unreserved_through() {
        // Exercises every branch of the url_encode match, including
        // the unreserved-set passthroughs (digits, dash, underscore,
        // dot, tilde) and the percent-escape fallback.
        let mut ctx = Context::new();
        ctx.set("v".to_string(), "Foo-Bar_42.~ x".to_string());
        let engine = Engine::new("", Duration::from_secs(60));
        let out = engine
            .render_template("{{ v | urlencode }}", &ctx)
            .unwrap();
        // Space becomes %20; everything else passes through.
        assert_eq!(out, "Foo-Bar_42.~%20x");
    }

    #[test]
    fn parse_filter_args_handles_quoted_commas() {
        // Exercises the in-quote path of parse_filter_args (commas
        // inside quotes don't split) plus the comma-separator path.
        let args = parse_filter_args(r#""a,b","c""#);
        assert_eq!(args, vec!["a,b", "c"]);
    }

    #[test]
    fn partial_params_single_quoted_value() {
        // Exercises the single-quote arm of parse_partial_value
        // (most tests use double quotes).
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "got={{label}}").unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine
            .render_template("{{> p label='hello'}}", &ctx)
            .unwrap();
        assert_eq!(out, "got=hello");
    }

    #[test]
    fn block_name_with_single_quotes_works_in_template() {
        // Exercises parse_block_name single-quote return through
        // the actual template path (#extends "name" tests cover
        // double quotes).
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "{{#block 'main'}}default{{/block}}",
        )
        .unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // The rendered base just yields the default body; the
        // important coverage is parse_block_name accepting 'main'.
        let out = engine
            .render_template(
                "{{#extends 'base'}}{{#block 'main'}}HI{{/block}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "HI");
    }

    #[test]
    fn unknown_byte_in_expression_consumed_silently() {
        // Tokenizer encountering an unknown byte advances past it
        // and returns None for that scan; the parser then hits
        // end-of-expression.
        let err = parse_expr("?;%").unwrap_err();
        // Just verify it errors cleanly rather than panicking.
        assert!(
            matches!(err, EngineError::InvalidTemplate(_)),
            "got {err:?}"
        );
    }

    // ── H7: Auto-escape per file extension ────────────────────────

    fn make_engine_with_two_pages() -> Engine {
        use std::collections::HashMap;
        let mut store = HashMap::new();
        let _ =
            store.insert("page.html".to_string(), "{{x}}".to_string());
        let _ =
            store.insert("plain.txt".to_string(), "{{x}}".to_string());
        Engine::with_loader(
            Arc::new(MemoryLoader::new(store)),
            Duration::from_secs(60),
        )
    }

    #[test]
    fn autoescape_on_escapes_for_listed_extension() {
        let mut engine = make_engine_with_two_pages();
        let _ = engine.autoescape_on(&[".html"]);
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "<b>".to_string());
        let out = engine.render_page(&ctx, "page.html").unwrap();
        assert_eq!(out, "&lt;b&gt;");
    }

    #[test]
    fn autoescape_on_skips_unlisted_extension() {
        let mut engine = make_engine_with_two_pages();
        let _ = engine.autoescape_on(&[".html"]);
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "<b>".to_string());
        let out = engine.render_page(&ctx, "plain.txt").unwrap();
        assert_eq!(out, "<b>");
    }

    #[test]
    fn autoescape_on_does_not_change_render_template_behaviour() {
        // render_template has no layout name; per-extension
        // policy doesn't apply — the global escape_html still rules.
        let mut engine = make_engine_with_two_pages();
        let _ = engine.autoescape_on(&[".html"]);
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "<b>".to_string());
        let out = engine.render_template("{{x}}", &ctx).unwrap();
        assert_eq!(out, "&lt;b&gt;");
    }

    #[test]
    fn autoescape_global_state_restored_after_render_page() {
        let mut engine = make_engine_with_two_pages();
        let _ = engine.autoescape_on(&[".html"]);
        let saved = engine.escape_html;
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let _ = engine.render_page(&ctx, "plain.txt").unwrap();
        assert_eq!(engine.escape_html, saved);
        let _ = engine.render_page(&ctx, "page.html").unwrap();
        assert_eq!(engine.escape_html, saved);
    }

    #[test]
    fn autoescape_multiple_extensions_match_any() {
        use std::collections::HashMap;
        let mut store = HashMap::new();
        let _ = store.insert("a.html".to_string(), "{{x}}".to_string());
        let _ = store.insert("b.xml".to_string(), "{{x}}".to_string());
        let _ = store.insert("c.txt".to_string(), "{{x}}".to_string());
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(store)),
            Duration::from_secs(60),
        );
        let _ = engine.autoescape_on(&[".html", ".xml"]);
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "<b>".to_string());
        assert_eq!(
            engine.render_page(&ctx, "a.html").unwrap(),
            "&lt;b&gt;"
        );
        assert_eq!(
            engine.render_page(&ctx, "b.xml").unwrap(),
            "&lt;b&gt;"
        );
        assert_eq!(engine.render_page(&ctx, "c.txt").unwrap(), "<b>");
    }

    // ── H6: json encode filter (feature-gated) ────────────────────

    #[cfg(feature = "json")]
    #[test]
    fn filter_json_encodes_strings() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada \"the\" Lovelace".to_string());
        let out = engine
            .render_template("{{ name | json | safe }}", &ctx)
            .unwrap();
        assert_eq!(out, r#""Ada \"the\" Lovelace""#);
    }

    #[cfg(feature = "json")]
    #[test]
    fn filter_json_encodes_numbers_and_bools() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 42i64);
        ctx.set_value("ok".to_string(), true);
        let n_out =
            engine.render_template("{{ n | json }}", &ctx).unwrap();
        assert_eq!(n_out, "42");
        let b_out =
            engine.render_template("{{ ok | json }}", &ctx).unwrap();
        assert_eq!(b_out, "true");
    }

    #[cfg(feature = "json")]
    #[test]
    fn filter_json_encodes_lists() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["alpha", "beta"]);
        let out = engine
            .render_template("{{ items | json | safe }}", &ctx)
            .unwrap();
        assert_eq!(out, r#"["alpha","beta"]"#);
    }

    #[cfg(feature = "json")]
    #[test]
    fn filter_json_encodes_nested_maps_with_sorted_keys() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        let mut user: fnv::FnvHashMap<String, crate::context::Value> =
            fnv::FnvHashMap::default();
        let _ = user.insert(
            "z_last".to_string(),
            crate::context::Value::Number(2),
        );
        let _ = user.insert(
            "a_first".to_string(),
            crate::context::Value::Number(1),
        );
        ctx.set_value(
            "user".to_string(),
            crate::context::Value::Map(user),
        );
        let out = engine
            .render_template("{{ user | json | safe }}", &ctx)
            .unwrap();
        // Keys must be in sorted order for deterministic output.
        assert_eq!(out, r#"{"a_first":1,"z_last":2}"#);
    }

    #[cfg(feature = "json")]
    #[test]
    fn filter_json_encodes_null_for_unset_path() {
        // Substituting a missing key normally errors, so test json
        // on an explicit Null value.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("x".to_string(), crate::context::Value::Null);
        let out =
            engine.render_template("{{ x | json }}", &ctx).unwrap();
        assert_eq!(out, "null");
    }

    // ── H5: Range iteration in #each ──────────────────────────────

    #[test]
    fn each_range_with_integer_literals() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template("{{#each 1..5}}[{{this}}]{{/each}}", &ctx)
            .unwrap();
        assert_eq!(out, "[1][2][3][4]");
    }

    #[test]
    fn each_range_starts_at_zero() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template("{{#each 0..3}}{{this}}{{/each}}", &ctx)
            .unwrap();
        assert_eq!(out, "012");
    }

    #[test]
    fn each_range_with_path_bounds() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("lo".to_string(), 2i64);
        ctx.set_value("hi".to_string(), 6i64);
        let out = engine
            .render_template("{{#each lo..hi}}{{this}},{{/each}}", &ctx)
            .unwrap();
        assert_eq!(out, "2,3,4,5,");
    }

    #[test]
    fn each_range_empty_when_lo_ge_hi() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#each 5..5}}should not appear{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn each_range_binds_index_and_helpers() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#each 0..3}}@{{@index}}={{this}};{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "@0=0;@1=1;@2=2;");
    }

    #[test]
    fn each_range_with_arithmetic_expression() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 3i64);
        let out = engine
            .render_template(
                "{{#each 0..n + 1}}{{this}}{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "0123");
    }

    #[test]
    fn each_range_errors_on_non_numeric_bounds() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("lo".to_string(), "abc".to_string());
        let err = engine
            .render_template("{{#each lo..5}}x{{/each}}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("range"), "{err}");
    }

    // ── H4: super() in inherited blocks ───────────────────────────

    #[test]
    fn super_in_child_block_includes_parent_default() {
        use std::collections::HashMap;
        let mut store = HashMap::new();
        let _ = store.insert(
            "base".to_string(),
            "[{{#block \"body\"}}DEFAULT{{/block}}]".to_string(),
        );
        let _ = store.insert(
            "child".to_string(),
            "{{#extends \"base\"}}\
             {{#block \"body\"}}( {{ super() }} )-OVR{{/block}}"
                .to_string(),
        );
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(store)),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine.render_page(&ctx, "child").unwrap();
        assert_eq!(out, "[( DEFAULT )-OVR]");
    }

    #[test]
    fn super_outside_override_is_silent_no_op() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out =
            engine.render_template("a{{ super() }}b", &ctx).unwrap();
        assert_eq!(out, "ab");
    }

    #[test]
    fn super_does_not_leak_across_partial_boundary() {
        use std::collections::HashMap;
        // Partial uses {{ super() }}; that should be a silent no-op
        // even when the partial is included from inside a block
        // override that has a non-empty super body.
        let mut store = HashMap::new();
        let _ = store.insert(
            "base".to_string(),
            "[{{#block \"main\"}}base{{/block}}]".to_string(),
        );
        let _ = store.insert(
            "child".to_string(),
            "{{#extends \"base\"}}\
             {{#block \"main\"}}{{> p}}{{/block}}"
                .to_string(),
        );
        let _ = store
            .insert("p".to_string(), "<{{ super() }}>".to_string());
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(store)),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine.render_page(&ctx, "child").unwrap();
        // super() inside p.html sees no parent body; emits nothing.
        assert_eq!(out, "[<>]");
    }

    #[test]
    fn super_renders_parent_body_with_parent_tags_resolved() {
        use std::collections::HashMap;
        // Parent block contains a tag that gets substituted from
        // the same context — verify super() renders the parent
        // body through the FULL pipeline (escape, dot-paths, etc).
        let mut store = HashMap::new();
        let _ = store.insert(
            "base".to_string(),
            "{{#block \"hi\"}}hello, {{name}}{{/block}}!".to_string(),
        );
        let _ = store.insert(
            "child".to_string(),
            "{{#extends \"base\"}}\
             {{#block \"hi\"}}>>> {{ super() }} <<<{{/block}}"
                .to_string(),
        );
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(store)),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine.render_page(&ctx, "child").unwrap();
        assert_eq!(out, ">>> hello, Ada <<<!");
    }

    // ── G3: TemplateLoader trait ───────────────────────────────────

    #[test]
    fn memory_loader_serves_render_page() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        let _ = map.insert(
            "greet".to_string(),
            "Hello, {{name}}!".to_string(),
        );
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(map)),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine.render_page(&ctx, "greet").unwrap();
        assert_eq!(out, "Hello, Ada!");
    }

    #[test]
    fn memory_loader_resolves_partials() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        let _ = map.insert(
            "main".to_string(),
            "outer {{> inner}} outer".to_string(),
        );
        let _ =
            map.insert("inner".to_string(), "<{{name}}>".to_string());
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(map)),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "x".to_string());
        let out = engine.render_page(&ctx, "main").unwrap();
        assert_eq!(out, "outer <x> outer");
    }

    #[test]
    fn memory_loader_resolves_extends_chain() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        let _ = map.insert(
            "base".to_string(),
            "[{{#block \"body\"}}default{{/block}}]".to_string(),
        );
        let _ = map.insert(
            "child".to_string(),
            "{{#extends \"base\"}}{{#block \"body\"}}OVR{{/block}}"
                .to_string(),
        );
        let mut engine = Engine::with_loader(
            Arc::new(MemoryLoader::new(map)),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine.render_page(&ctx, "child").unwrap();
        assert_eq!(out, "[OVR]");
    }

    #[test]
    fn memory_loader_missing_template_yields_io_error() {
        let engine = Engine::with_loader(
            Arc::new(MemoryLoader::default()),
            Duration::from_secs(60),
        );
        let mut engine = engine;
        let ctx = Context::new();
        let err = engine.render_page(&ctx, "missing").unwrap_err();
        assert!(matches!(err, EngineError::Io(_)), "{err:?}");
    }

    #[test]
    fn memory_loader_insert_replaces_template() {
        let mut loader = MemoryLoader::default();
        let prev = loader.insert("k", "first");
        assert!(prev.is_none());
        let prev = loader.insert("k", "second");
        assert_eq!(prev.as_deref(), Some("first"));
        assert_eq!(loader.load("k").unwrap(), "second");
    }

    #[test]
    fn fs_loader_serves_real_files() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("page.html"), "fs body {{x}}")
            .unwrap();
        let loader = FsLoader::new(temp.path().to_path_buf());
        assert_eq!(loader.load("page").unwrap(), "fs body {{x}}");
    }

    #[test]
    fn engine_new_uses_fs_loader_by_default() {
        // Backwards-compat: render_page on the original constructor
        // still walks the filesystem.
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "default {{w}}").unwrap();
        let mut engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("w".to_string(), "world".to_string());
        let out = engine.render_page(&ctx, "p").unwrap();
        assert_eq!(out, "default world");
    }

    // ── G2: String filters ─────────────────────────────────────────

    #[test]
    fn filter_repeat_repeats_string_n_times() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("c".to_string(), "ab".to_string());
        let out =
            engine.render_template("{{ c | repeat:3 }}", &ctx).unwrap();
        assert_eq!(out, "ababab");
    }

    #[test]
    fn filter_repeat_zero_returns_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("c".to_string(), "x".to_string());
        let out =
            engine.render_template("{{ c | repeat:0 }}", &ctx).unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn filter_repeat_errors_without_arg() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("c".to_string(), "x".to_string());
        let err = engine
            .render_template("{{ c | repeat }}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("repeat"), "{err}");
    }

    #[test]
    fn filter_reverse_unicode_aware() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "café".to_string());
        let out =
            engine.render_template("{{ s | reverse }}", &ctx).unwrap();
        assert_eq!(out, "éfac");
    }

    #[test]
    fn filter_slice_with_start_and_end() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "abcdef".to_string());
        let out = engine
            .render_template("{{ s | slice:1,4 }}", &ctx)
            .unwrap();
        assert_eq!(out, "bcd");
    }

    #[test]
    fn filter_slice_with_only_start() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "abcdef".to_string());
        let out =
            engine.render_template("{{ s | slice:2 }}", &ctx).unwrap();
        assert_eq!(out, "cdef");
    }

    #[test]
    fn filter_slice_clamps_oversize_indices() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "ab".to_string());
        let out = engine
            .render_template("{{ s | slice:100,200 }}", &ctx)
            .unwrap();
        assert_eq!(out, "");
    }

    #[test]
    fn filter_slice_errors_without_start() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "ab".to_string());
        let err = engine
            .render_template("{{ s | slice }}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("slice"), "{err}");
    }

    #[test]
    fn filter_pad_start_right_aligns_with_spaces() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "42".to_string());
        let out = engine
            .render_template("{{ s | pad_start:5 }}", &ctx)
            .unwrap();
        assert_eq!(out, "   42");
    }

    #[test]
    fn filter_pad_end_left_aligns_with_custom_char() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "x".to_string());
        let out = engine
            .render_template(r#"{{ s | pad_end:4,"." }}"#, &ctx)
            .unwrap();
        assert_eq!(out, "x...");
    }

    #[test]
    fn filter_pad_returns_input_unchanged_when_already_long() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "abcdef".to_string());
        let out = engine
            .render_template("{{ s | pad_start:3 }}", &ctx)
            .unwrap();
        assert_eq!(out, "abcdef");
    }

    #[test]
    fn filter_pad_errors_without_width() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "x".to_string());
        let err = engine
            .render_template("{{ s | pad_start }}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("pad"), "{err}");
    }

    #[test]
    fn filter_contains_emits_true_or_false() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "hello world".to_string());
        let yes = engine
            .render_template(r#"{{ s | contains:"world" }}"#, &ctx)
            .unwrap();
        assert_eq!(yes, "true");
        let no = engine
            .render_template(r#"{{ s | contains:"xyz" }}"#, &ctx)
            .unwrap();
        assert_eq!(no, "false");
    }

    #[test]
    fn filter_starts_with_and_ends_with() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "abc".to_string());
        assert_eq!(
            engine
                .render_template(r#"{{ s | starts_with:"a" }}"#, &ctx)
                .unwrap(),
            "true"
        );
        assert_eq!(
            engine
                .render_template(r#"{{ s | ends_with:"c" }}"#, &ctx)
                .unwrap(),
            "true"
        );
        assert_eq!(
            engine
                .render_template(r#"{{ s | starts_with:"z" }}"#, &ctx)
                .unwrap(),
            "false"
        );
    }

    #[test]
    fn filter_contains_no_arg_returns_false() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("s".to_string(), "abc".to_string());
        let out =
            engine.render_template("{{ s | contains }}", &ctx).unwrap();
        assert_eq!(out, "false");
    }

    // ── G1: Loop break / continue ─────────────────────────────────

    #[test]
    fn each_break_stops_iteration() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c", "d"]);
        // Stop emitting once we've seen "b".
        let out = engine
            .render_template(
                "{{#each items}}\
                 {{#if this == \"b\"}}{{#break}}{{/if}}\
                 [{{this}}]\
                 {{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "[a]");
    }

    #[test]
    fn each_continue_skips_iteration() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
        // Skip "b", emit the rest.
        let out = engine
            .render_template(
                "{{#each items}}\
                 {{#if this == \"b\"}}{{#continue}}{{/if}}\
                 [{{this}}]\
                 {{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "[a][c]");
    }

    #[test]
    fn break_propagates_through_nested_if() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["x", "y", "z"]);
        // The break is inside two nested #if blocks; should still
        // bubble up to the each loop.
        let out = engine
            .render_template(
                "{{#each items}}\
                 {{#if true}}{{#if this == \"y\"}}{{#break}}{{/if}}{{/if}}\
                 [{{this}}]\
                 {{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "[x]");
    }

    #[test]
    fn break_in_one_each_does_not_affect_outer_loop() {
        // An inner #break should only short-circuit its own #each,
        // not the outer one.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("outer".to_string(), vec!["A", "B"]);
        ctx.set_value("inner".to_string(), vec!["1", "2", "3"]);
        let out = engine
            .render_template(
                "{{#each outer}}\
                 [{{this}}:\
                 {{#each inner}}\
                 {{#if this == \"2\"}}{{#break}}{{/if}}\
                 {{this}}\
                 {{/each}}\
                 ]\
                 {{/each}}",
                &ctx,
            )
            .unwrap();
        // Inner each emits "1" then breaks at "2"; outer keeps going.
        assert_eq!(out, "[A:1][B:1]");
    }

    #[test]
    fn break_at_top_level_is_silent_no_op() {
        // `#break` outside any loop should not error — the signal
        // bubbles to render_template and is discarded.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template("before{{#break}}after", &ctx)
            .unwrap();
        // Render stops at #break; "after" is not emitted because
        // render_recursive returned early.
        assert_eq!(out, "before");
    }

    // ── F4: Custom tests API ───────────────────────────────────────

    #[test]
    fn add_test_registers_custom_predicate() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_test(
            "admin",
            Arc::new(|v, _args| {
                Ok(matches!(v, crate::context::Value::String(s) if s == "admin"))
            }),
        );
        let mut ctx = Context::new();
        ctx.set("role".to_string(), "admin".to_string());
        let out = engine
            .render_template(
                "{{#if role is admin}}Y{{else}}N{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "Y");

        ctx.set("role".to_string(), "guest".to_string());
        let out = engine
            .render_template(
                "{{#if role is admin}}Y{{else}}N{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "N");
    }

    #[test]
    fn add_test_supports_is_not_negation() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_test(
            "even",
            Arc::new(|v, _args| match v {
                crate::context::Value::Number(n) => Ok(n % 2 == 0),
                _ => Ok(false),
            }),
        );
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 3i64);
        let out = engine
            .render_template(
                "{{#if n is not even}}odd{{else}}even{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "odd");
    }

    #[test]
    fn add_test_unknown_name_still_errors() {
        // After F4, unknown test names defer to eval. If they're
        // not registered there either, eval surfaces the error.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "1".to_string());
        let err = engine
            .render_template("{{#if x is missing}}y{{/if}}", &ctx)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown test"), "{msg}");
        assert!(msg.contains("missing"), "{msg}");
    }

    #[test]
    fn add_test_overrides_builtin_with_same_name() {
        // Override `defined` to always return false — proves
        // the custom-tests lookup happens BEFORE built-in dispatch.
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ =
            engine.add_test("defined", Arc::new(|_v, _args| Ok(false)));
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let out = engine
            .render_template(
                "{{#if x is defined}}Y{{else}}N{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "N");
    }

    #[test]
    fn add_test_propagates_user_errors() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_test(
            "boom",
            Arc::new(|_v, _args| {
                Err(EngineError::Render("test exploded".to_string()))
            }),
        );
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let err = engine
            .render_template("{{#if x is boom}}y{{/if}}", &ctx)
            .unwrap_err();
        assert!(format!("{err}").contains("test exploded"), "{err}");
    }

    // ── F3: Number formatting filters ─────────────────────────────

    #[test]
    fn filter_abs_strips_sign() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), -42i64);
        let out =
            engine.render_template("{{ n | abs }}", &ctx).unwrap();
        assert_eq!(out, "42");
    }

    #[test]
    fn filter_round_to_integer() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("price".to_string(), "3.6".to_string());
        let out = engine
            .render_template("{{ price | round }}", &ctx)
            .unwrap();
        assert_eq!(out, "4");
    }

    #[test]
    fn filter_ceil_rounds_up() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("v".to_string(), "2.1".to_string());
        let out =
            engine.render_template("{{ v | ceil }}", &ctx).unwrap();
        assert_eq!(out, "3");
    }

    #[test]
    fn filter_floor_rounds_down() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("v".to_string(), "2.9".to_string());
        let out =
            engine.render_template("{{ v | floor }}", &ctx).unwrap();
        assert_eq!(out, "2");
    }

    #[test]
    fn filter_round_on_integer_is_identity() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 5i64);
        let out =
            engine.render_template("{{ n | round }}", &ctx).unwrap();
        assert_eq!(out, "5");
    }

    #[test]
    fn filter_number_format_default_comma() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 1_234_567i64);
        let out = engine
            .render_template("{{ n | number_format }}", &ctx)
            .unwrap();
        assert_eq!(out, "1,234,567");
    }

    #[test]
    fn filter_number_format_custom_separator() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 1_234_567i64);
        let out = engine
            .render_template(r#"{{ n | number_format:"_" }}"#, &ctx)
            .unwrap();
        assert_eq!(out, "1_234_567");
    }

    #[test]
    fn filter_number_format_handles_negative_and_decimals() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("p".to_string(), "-1234567.89".to_string());
        let out = engine
            .render_template("{{ p | number_format }}", &ctx)
            .unwrap();
        assert_eq!(out, "-1,234,567.89");
    }

    #[test]
    fn filter_number_format_short_number_unchanged() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 42i64);
        let out = engine
            .render_template("{{ n | number_format }}", &ctx)
            .unwrap();
        assert_eq!(out, "42");
    }

    #[test]
    fn filter_round_on_non_number_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let err = engine
            .render_template("{{ name | round }}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err}").contains("expected a number"),
            "got {err}"
        );
    }

    #[test]
    fn filter_number_format_on_non_number_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let err = engine
            .render_template("{{ name | number_format }}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err}").contains("expected a number"),
            "got {err}"
        );
    }

    // ── F2: String concatenation operator (~) ─────────────────────

    #[test]
    fn if_concat_two_string_literals() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                r#"{{#if "foo" ~ "bar" == "foobar"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn if_concat_path_and_literal() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let out = engine
            .render_template(
                r#"{{#if name ~ " Lovelace" == "Ada Lovelace"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn if_concat_coerces_number_to_string() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("n".to_string(), 5);
        let out = engine
            .render_template(
                r#"{{#if n ~ "x" == "5x"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn if_concat_left_associates() {
        // "a" ~ "b" ~ "c" must parse as (("a" ~ "b") ~ "c") and produce "abc".
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                r#"{{#if "a" ~ "b" ~ "c" == "abc"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn if_concat_binds_tighter_than_comparison() {
        // `a ~ b == c ~ d` must parse as `(a ~ b) == (c ~ d)`.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                r#"{{#if "a" ~ "b" == "a" ~ "b"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn if_concat_null_renders_as_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("nope".to_string(), crate::context::Value::Null);
        let out = engine
            .render_template(
                r#"{{#if nope ~ "x" == "x"}}y{{else}}n{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "y");
    }

    #[test]
    fn malformed_block_name_reports_line_number() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // Empty block name should fail name parsing.
        let template = "x\n{{#block }}body{{/block}}";
        let err = engine.render_template(template, &ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 2"), "got: {msg}");
    }

    #[test]
    fn render_to_writes_into_a_vec() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        let mut buf: Vec<u8> = Vec::new();
        engine
            .render_to("Hello, {{name}}!", &ctx, &mut buf)
            .unwrap();
        assert_eq!(buf, b"Hello, Ada!");
    }

    #[test]
    fn render_to_matches_render_template_byte_for_byte() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("title".to_string(), "Posts".to_string());
        ctx.set_value(
            "items".to_string(),
            vec!["alpha", "beta", "gamma"],
        );
        let template = "<h1>{{title | uppercase}}</h1>\
                        <ul>{{#each items}}<li>{{this}}</li>{{/each}}</ul>";
        let direct = engine.render_template(template, &ctx).unwrap();
        let mut streamed: Vec<u8> = Vec::new();
        engine.render_to(template, &ctx, &mut streamed).unwrap();
        assert_eq!(streamed, direct.into_bytes());
    }

    #[test]
    fn render_to_propagates_io_errors() {
        // A writer that always fails proves errors map to EngineError::Io.
        struct Bomb;
        impl Write for Bomb {
            fn write(&mut self, _b: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, "no"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let err =
            engine.render_to("{{x}}", &ctx, &mut Bomb).unwrap_err();
        assert!(matches!(err, EngineError::Io(_)));
    }

    #[test]
    fn render_to_propagates_template_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let mut buf: Vec<u8> = Vec::new();
        let err = engine
            .render_to("{{missing}}", &ctx, &mut buf)
            .unwrap_err();
        assert!(matches!(err, EngineError::Render(_)));
        // Writer was not partially written when render fails first.
        assert!(buf.is_empty());
    }

    #[test]
    fn add_filter_chains_with_builtins() {
        use std::sync::Arc;
        let mut engine = Engine::new("", Duration::from_secs(60));
        let _ = engine.add_filter(
            "exclaim",
            Arc::new(|input, _args| Ok(format!("{input}!"))),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "  ada  ".to_string());
        // built-in `trim` -> custom `exclaim` -> built-in `uppercase`
        let out = engine
            .render_template(
                "{{ name | trim | exclaim | uppercase }}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "ADA!");
    }

    #[test]
    fn each_binds_index_first_last_for_lists() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
        let out = engine
            .render_template(
                "{{#each items}}[{{@index}}={{this}} f={{@first}} l={{@last}}]{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(
            out,
            "[0=a f=true l=false][1=b f=false l=false][2=c f=false l=true]",
        );
    }

    #[test]
    fn each_iterates_maps_with_key_binding() {
        use crate::context::Value;
        use fnv::FnvHashMap;

        let engine = Engine::new("", Duration::from_secs(60));
        let mut prefs = FnvHashMap::default();
        let _ = prefs.insert("color".to_string(), Value::from("blue"));
        let _ = prefs.insert("size".to_string(), Value::from("M"));
        let mut ctx = Context::new();
        ctx.set_value("prefs".to_string(), Value::Map(prefs));

        // Map iteration is sorted by key for deterministic output.
        let out = engine
            .render_template(
                "{{#each prefs}}{{@key}}={{this}} {{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "color=blue size=M ");
    }

    #[test]
    fn if_compares_numbers_with_gt_lt_eq() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("count".to_string(), 7);

        for (expr, expected) in [
            ("count > 5", "yes"),
            ("count >= 7", "yes"),
            ("count < 5", "no"),
            ("count <= 6", "no"),
            ("count == 7", "yes"),
            ("count != 7", "no"),
        ] {
            let tpl = format!(
                "{{{{#if {expr}}}}}yes{{{{else}}}}no{{{{/if}}}}"
            );
            let out = engine.render_template(&tpl, &ctx).unwrap();
            assert_eq!(out, expected, "expr `{expr}`");
        }
    }

    #[test]
    fn if_compares_string_literals() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("role".to_string(), "admin".to_string());
        let out = engine
            .render_template(
                r#"{{#if role == "admin"}}A{{else}}U{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "A");

        let out = engine
            .render_template(
                r#"{{#if role != "guest"}}A{{else}}U{{/if}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "A");
    }

    #[test]
    fn if_compares_two_paths() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("a".to_string(), 5);
        ctx.set_value("b".to_string(), 5);
        let out = engine
            .render_template("{{#if a == b}}eq{{else}}ne{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "eq");
    }

    #[test]
    fn if_orders_strings_lexically() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("a".to_string(), "apple".to_string());
        ctx.set("b".to_string(), "banana".to_string());
        let out = engine
            .render_template("{{#if a < b}}lt{{else}}ge{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "lt");
    }

    #[test]
    fn if_bare_path_keeps_truthiness_semantics() {
        // Backwards-compat: `{{#if x}}` without an operator still
        // evaluates to truthy / falsy.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("on".to_string(), true);
        ctx.set_value("off".to_string(), false);
        ctx.set_value("zero".to_string(), 0);
        ctx.set_value("seven".to_string(), 7);

        for (expr, expected) in [
            ("on", "Y"),
            ("off", "N"),
            ("zero", "N"),
            ("seven", "Y"),
            ("missing", "N"),
        ] {
            let tpl =
                format!("{{{{#if {expr}}}}}Y{{{{else}}}}N{{{{/if}}}}");
            assert_eq!(
                engine.render_template(&tpl, &ctx).unwrap(),
                expected,
                "expr `{expr}`",
            );
        }
    }

    #[test]
    fn if_ordered_compare_on_mixed_types_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("count".to_string(), 7);
        ctx.set("name".to_string(), "Ada".to_string());
        let err = engine
            .render_template("{{#if count > name}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::InvalidTemplate(msg) if msg.contains("must be numbers or both strings"),
        ));
    }

    #[test]
    fn if_eq_works_across_types_via_structural_equality() {
        // Eq/Ne don't require type matching — true vs 1 are simply
        // unequal under structural Value equality.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("on".to_string(), true);
        let out = engine
            .render_template(
                "{{#if on == 1}}same{{else}}diff{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "diff");
    }

    #[test]
    fn if_compares_against_null_literal() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("x".to_string(), crate::context::Value::Null);
        let out = engine
            .render_template(
                "{{#if x == null}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    // ── Phase C2: boolean operators ────────────────────────────────

    #[test]
    fn if_combines_with_and_short_circuits_truth() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Bool(true),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(true),
        );
        let out = engine
            .render_template(
                "{{#if a and b}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");

        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(false),
        );
        let out = engine
            .render_template(
                "{{#if a and b}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "no");
    }

    #[test]
    fn if_combines_with_or_picks_truthy_branch() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Bool(false),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(true),
        );
        let out = engine
            .render_template("{{#if a or b}}yes{{else}}no{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "yes");

        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(false),
        );
        let out = engine
            .render_template("{{#if a or b}}yes{{else}}no{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "no");
    }

    #[test]
    fn if_negates_with_not() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "x".to_string(),
            crate::context::Value::Bool(false),
        );
        let out = engine
            .render_template("{{#if not x}}yes{{else}}no{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_double_negates_with_not_not() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "x".to_string(),
            crate::context::Value::Bool(true),
        );
        let out = engine
            .render_template(
                "{{#if not not x}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_respects_not_over_and_over_or_precedence() {
        // `not a and b or c` must parse as `((not a) and b) or c`.
        // With a=true, b=true, c=false → ((false) and true) or false
        // → false. With a=false, b=true, c=false → ((true) and true)
        // or false → true. Same template, different context.
        let engine = Engine::new("", Duration::from_secs(60));
        let template = "{{#if not a and b or c}}yes{{else}}no{{/if}}";

        let mut ctx = Context::new();
        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Bool(true),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(true),
        );
        ctx.set_value(
            "c".to_string(),
            crate::context::Value::Bool(false),
        );
        assert_eq!(
            engine.render_template(template, &ctx).unwrap(),
            "no"
        );

        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Bool(false),
        );
        assert_eq!(
            engine.render_template(template, &ctx).unwrap(),
            "yes"
        );
    }

    #[test]
    fn if_combines_comparisons_with_and_or() {
        // Comparisons bind tighter than boolean ops.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "n".to_string(),
            crate::context::Value::Number(7),
        );
        ctx.set_value(
            "name".to_string(),
            crate::context::Value::String("Ada".into()),
        );
        let out = engine
            .render_template(
                "{{#if n > 5 and name == \"Ada\"}}match{{else}}miss{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "match");
    }

    #[test]
    fn if_or_short_circuits_past_unbound_path() {
        // The right-hand operand should not even be evaluated when the
        // left is already truthy. `missing` would resolve to Null
        // either way (not an error), but this protects the contract.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "ready".to_string(),
            crate::context::Value::Bool(true),
        );
        let out = engine
            .render_template(
                "{{#if ready or missing}}go{{else}}wait{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "go");
    }

    #[test]
    fn if_dotted_path_with_and_substring_is_not_keyword() {
        // The keyword detection only fires on standalone identifiers,
        // so `user.notes` is still a valid path.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        let mut user: fnv::FnvHashMap<String, crate::context::Value> =
            fnv::FnvHashMap::default();
        let _ = user.insert(
            "notes".to_string(),
            crate::context::Value::String("hi".into()),
        );
        ctx.set_value(
            "user".to_string(),
            crate::context::Value::Map(user),
        );
        let out = engine
            .render_template(
                "{{#if user.notes}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    // ── Phase C3: integer math operators ───────────────────────────

    #[test]
    fn if_compares_addition_against_threshold() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "n".to_string(),
            crate::context::Value::Number(8),
        );
        let out = engine
            .render_template(
                "{{#if n + 2 == 10}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_subtracts_two_paths() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Number(20),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Number(7),
        );
        let out = engine
            .render_template(
                "{{#if a - b > 10}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_respects_mul_over_add_precedence() {
        // 2 + 3 * 4 must equal 14, not 20.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#if 2 + 3 * 4 == 14}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_evaluates_left_associatively_for_subtraction() {
        // 10 - 3 - 2 must equal 5, not 9.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#if 10 - 3 - 2 == 5}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_does_integer_division_truncating_toward_zero() {
        // 7 / 2 == 3 (integer), not 3.5.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#if 7 / 2 == 3}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_division_by_zero_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "z".to_string(),
            crate::context::Value::Number(0),
        );
        let err = engine
            .render_template("{{#if 5 / z == 0}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("division by zero"),
            "expected division-by-zero error, got: {err:?}"
        );
    }

    #[test]
    fn if_math_on_non_numbers_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "name".to_string(),
            crate::context::Value::String("Ada".into()),
        );
        let err = engine
            .render_template("{{#if name + 1 == 1}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("math operator requires"),
            "expected math-type error, got: {err:?}"
        );
    }

    #[test]
    fn if_negative_literal_still_parses() {
        // `-3` after `>` (a non-operand token) should tokenize as a
        // negative number literal, not as `Minus, 3`.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "n".to_string(),
            crate::context::Value::Number(-5),
        );
        let out = engine
            .render_template("{{#if n < -3}}yes{{else}}no{{/if}}", &ctx)
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_math_chains_with_boolean_ops() {
        // Math < comparisons < booleans in precedence.
        // `(a + b) > 10 and (c * 2) <= 8`
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "a".to_string(),
            crate::context::Value::Number(7),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Number(5),
        );
        ctx.set_value(
            "c".to_string(),
            crate::context::Value::Number(3),
        );
        let out = engine
            .render_template(
                "{{#if a + b > 10 and c * 2 <= 8}}\
                 ok{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "ok");
    }

    #[test]
    fn if_integer_overflow_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "max".to_string(),
            crate::context::Value::Number(i64::MAX),
        );
        let err = engine
            .render_template("{{#if max + 1 == 0}}x{{/if}}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("integer overflow"),
            "expected overflow error, got: {err:?}"
        );
    }

    // ── Phase C4: postfix tests (is defined / empty / none) ────────

    #[test]
    fn if_is_defined_true_for_present_path() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "name".to_string(),
            crate::context::Value::String("Ada".into()),
        );
        let out = engine
            .render_template(
                "{{#if name is defined}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_is_defined_false_for_missing_path() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#if missing is defined}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "no");
    }

    #[test]
    fn if_is_defined_true_for_explicit_null() {
        // Explicitly setting a key to Null still counts as defined,
        // because the key exists. This matches Tera/Jinja semantics
        // where `is defined` answers "does the variable exist".
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("x".to_string(), crate::context::Value::Null);
        let out = engine
            .render_template(
                "{{#if x is defined}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_is_not_defined_negates() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#if missing is not defined}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn if_is_empty_true_for_empty_string_list_map_null() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "s".to_string(),
            crate::context::Value::String(String::new()),
        );
        ctx.set_value(
            "l".to_string(),
            crate::context::Value::List(vec![]),
        );
        ctx.set_value(
            "m".to_string(),
            crate::context::Value::Map(fnv::FnvHashMap::default()),
        );
        ctx.set_value("z".to_string(), crate::context::Value::Null);
        for key in ["s", "l", "m", "z"] {
            let out = engine
                .render_template(
                    &format!(
                        "{{{{#if {key} is empty}}}}yes{{{{else}}}}no{{{{/if}}}}"
                    ),
                    &ctx,
                )
                .unwrap();
            assert_eq!(out, "yes", "{key} should be empty");
        }
    }

    #[test]
    fn if_is_empty_false_for_zero_and_false_and_nonempty_string() {
        // Numbers and bools are never empty — `0 is empty` is false
        // even though `0` is falsy. Tests probe a specific shape;
        // truthiness is a separate axis.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "n".to_string(),
            crate::context::Value::Number(0),
        );
        ctx.set_value(
            "b".to_string(),
            crate::context::Value::Bool(false),
        );
        ctx.set_value(
            "s".to_string(),
            crate::context::Value::String("hi".into()),
        );
        for key in ["n", "b", "s"] {
            let out = engine
                .render_template(
                    &format!(
                        "{{{{#if {key} is empty}}}}yes{{{{else}}}}no{{{{/if}}}}"
                    ),
                    &ctx,
                )
                .unwrap();
            assert_eq!(out, "no", "{key} should not be empty");
        }
    }

    #[test]
    fn if_is_none_true_only_for_null() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("z".to_string(), crate::context::Value::Null);
        ctx.set_value(
            "s".to_string(),
            crate::context::Value::String(String::new()),
        );
        let yes = engine
            .render_template(
                "{{#if z is none}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(yes, "yes");
        let no = engine
            .render_template(
                "{{#if s is none}}yes{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(no, "no");
    }

    #[test]
    fn if_is_not_none_negates() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "x".to_string(),
            crate::context::Value::Number(5),
        );
        let out = engine
            .render_template(
                "{{#if x is not none}}set{{else}}null{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "set");
    }

    #[test]
    fn if_unknown_test_name_errors_cleanly() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "x".to_string(),
            crate::context::Value::Bool(true),
        );
        let err = engine
            .render_template("{{#if x is bogus}}y{{/if}}", &ctx)
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("unknown test"),
            "expected unknown-test error, got: {err:?}"
        );
    }

    #[test]
    fn if_test_combines_with_boolean_ops() {
        // `name is defined and name is not empty` is the canonical
        // "has a non-blank value" check.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value(
            "name".to_string(),
            crate::context::Value::String("Ada".into()),
        );
        let out = engine
            .render_template(
                "{{#if name is defined and name is not empty}}\
                 ok{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "ok");

        ctx.set_value(
            "name".to_string(),
            crate::context::Value::String(String::new()),
        );
        let out = engine
            .render_template(
                "{{#if name is defined and name is not empty}}\
                 ok{{else}}no{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "no");
    }

    #[test]
    fn if_dotted_path_can_test_for_definedness() {
        // `user.email is defined` walks the dotted path before
        // running the test.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        let mut user: fnv::FnvHashMap<String, crate::context::Value> =
            fnv::FnvHashMap::default();
        let _ = user.insert(
            "email".to_string(),
            crate::context::Value::String("a@b".into()),
        );
        ctx.set_value(
            "user".to_string(),
            crate::context::Value::Map(user),
        );
        let yes = engine
            .render_template(
                "{{#if user.email is defined}}y{{else}}n{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(yes, "y");
        let no = engine
            .render_template(
                "{{#if user.phone is defined}}y{{else}}n{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(no, "n");
    }

    #[test]
    fn set_binds_string_literal_for_subsequent_tags() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                r#"{{#set name = "Ada"}}Hello, {{name}}!"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "Hello, Ada!");
    }

    #[test]
    fn set_binds_integer_bool_and_null_literals() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#set n = 42}}{{#set ok = true}}{{#set z = null}}\
                 n={{n}} ok={{ok}} z={{z}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "n=42 ok=true z=");
    }

    #[test]
    fn set_does_not_mutate_caller_context() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "outer".to_string());

        let _ = engine
            .render_template(r#"{{#set name = "inner"}}{{name}}"#, &ctx)
            .unwrap();

        // Caller's context is unchanged after rendering.
        assert_eq!(ctx.get("name"), Some(&"outer".to_string()),);
    }

    #[test]
    fn set_in_each_body_does_not_leak_across_iterations() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
        // Each iteration starts fresh — `marker` is set inside the body
        // but the parent context never sees it after the loop.
        let out = engine
            .render_template(
                r#"{{#each items}}{{#set marker = "X"}}{{this}}={{marker}} {{/each}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "a=X b=X c=X ");
    }

    #[test]
    fn set_visible_inside_subsequent_if_block() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                "{{#set ready = true}}{{#if ready}}go{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "go");
    }

    #[test]
    fn set_supports_dot_notation_on_left_side() {
        // `#set` only takes a flat key — dot-notation lookups still
        // work because the bound key matches verbatim.
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let out = engine
            .render_template(
                r#"{{#set greeting = "hi"}}{{greeting}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "hi");
    }

    #[test]
    fn set_missing_value_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err =
            engine.render_template("{{#set x =}}", &ctx).unwrap_err();
        assert!(matches!(
            err,
            EngineError::InvalidTemplate(msg) if msg.contains("missing value"),
        ));
    }

    #[test]
    fn set_missing_equals_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        let err =
            engine.render_template("{{#set x}}", &ctx).unwrap_err();
        assert!(matches!(
            err,
            EngineError::InvalidTemplate(msg) if msg.contains("`= value`"),
        ));
    }

    #[test]
    fn extends_overrides_named_blocks_in_base() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "<title>{{#block \"title\"}}Default{{/block}}</title>",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let child = "{{#extends \"base\"}}\
                     {{#block \"title\"}}Custom{{/block}}";

        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "<title>Custom</title>");
    }

    #[test]
    fn extends_falls_back_to_default_when_block_not_overridden() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "[{{#block \"a\"}}A-default{{/block}}]\
             [{{#block \"b\"}}B-default{{/block}}]",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // Child only overrides `b`; `a` uses its default body.
        let child = "{{#extends \"base\"}}\
                     {{#block \"b\"}}B-custom{{/block}}";

        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "[A-default][B-custom]");
    }

    #[test]
    fn extends_supports_nested_blocks_with_partial_overrides() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "<head>\
               {{#block \"head\"}}\
                 <title>{{#block \"title\"}}Default{{/block}}</title>\
               {{/block}}\
             </head>",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // Override only the inner block; outer falls back to default,
        // which contains the now-overridden inner block.
        let child = "{{#extends \"base\"}}\
                     {{#block \"title\"}}Custom{{/block}}";

        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "<head><title>Custom</title></head>");
    }

    #[test]
    fn extends_chains_through_multiple_levels() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "{{#block \"x\"}}base-x{{/block}}",
        )
        .unwrap();
        fs::write(
            temp.path().join("middle.html"),
            "{{#extends \"base\"}}\
             {{#block \"x\"}}middle-x{{/block}}",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // Child overrides x; should win over middle's x.
        let child = "{{#extends \"middle\"}}\
                     {{#block \"x\"}}child-x{{/block}}";

        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "child-x");
    }

    #[test]
    fn extends_chain_uses_middle_when_child_does_not_override() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "{{#block \"x\"}}base-x{{/block}}",
        )
        .unwrap();
        fs::write(
            temp.path().join("middle.html"),
            "{{#extends \"base\"}}\
             {{#block \"x\"}}middle-x{{/block}}",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // Child extends middle but doesn't override x — middle's x wins.
        let child = "{{#extends \"middle\"}}";
        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "middle-x");
    }

    #[test]
    fn extends_circular_chain_errors_at_depth_limit() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        // a extends b extends a → infinite chain capped by MAX_RENDER_DEPTH.
        fs::write(temp.path().join("a.html"), "{{#extends \"b\"}}")
            .unwrap();
        fs::write(temp.path().join("b.html"), "{{#extends \"a\"}}")
            .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let res = engine.render_template("{{#extends \"a\"}}", &ctx);
        assert!(matches!(
            res,
            Err(EngineError::Render(msg)) if msg.contains("recursion depth"),
        ));
    }

    #[test]
    fn extends_drops_literal_text_outside_blocks_in_child() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "[{{#block \"x\"}}default{{/block}}]",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        // Literal text "ignored garbage" between extends and the block
        // contributes nothing to the output.
        let child = "{{#extends \"base\"}}ignored garbage\
                     {{#block \"x\"}}custom{{/block}}\
                     more ignored garbage";
        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "[custom]");
    }

    #[test]
    fn block_name_accepts_bareword_and_quoted_forms() {
        // `parse_block_name` strips matching single/double quotes and
        // returns the inner. Bareword passes through.
        assert_eq!(parse_block_name("title").unwrap(), "title");
        assert_eq!(parse_block_name("\"title\"").unwrap(), "title");
        assert_eq!(parse_block_name("'title'").unwrap(), "title");
        assert!(parse_block_name("").is_err());
    }

    #[test]
    fn block_inside_each_renders_per_iteration() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("base.html"),
            "{{#each items}}\
               [{{#block \"item\"}}{{this}}{{/block}}]\
             {{/each}}",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
        let child = "{{#extends \"base\"}}\
                     {{#block \"item\"}}<{{this}}>{{/block}}";

        let out = engine.render_template(child, &ctx).unwrap();
        assert_eq!(out, "[<a>][<b>][<c>]");
    }

    #[test]
    fn partial_with_named_params_overrides_context() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("badge.html"),
            "{{label}}={{value}}",
        )
        .unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("label".to_string(), "outer".to_string());

        // The partial sees label="version", value=42 — outer.label
        // is shadowed only inside this invocation.
        let out = engine
            .render_template(
                r#"{{> badge label="version" value=42}} {{label}}"#,
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "version=42 outer");
    }

    #[test]
    fn partial_param_handles_quoted_string_with_spaces() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("greet.html"), "Hello, {{name}}!")
            .unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine
            .render_template(r#"{{> greet name="Ada Lovelace"}}"#, &ctx)
            .unwrap();
        assert_eq!(out, "Hello, Ada Lovelace!");
    }

    #[test]
    fn partial_param_unclosed_quote_errors() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("p.html"), "x").unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let err = engine
            .render_template(r#"{{> p name="open}}"#, &ctx)
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::InvalidTemplate(msg) if msg.contains("unclosed quote"),
        ));
    }

    #[test]
    fn partial_param_recognises_booleans_and_null() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("flags.html"),
            "{{#if active}}A{{else}}-{{/if}} {{count}}",
        )
        .unwrap();
        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let ctx = Context::new();
        let out = engine
            .render_template("{{> flags active=true count=3}}", &ctx)
            .unwrap();
        assert_eq!(out, "A 3");
    }

    #[test]
    fn each_on_empty_list_renders_nothing() {
        use crate::context::Value;
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("xs".to_string(), Value::List(Vec::new()));
        let out = engine
            .render_template("[{{#each xs}}x{{/each}}]", &ctx)
            .unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn filters_compose_in_chain() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("title".to_string(), "  hello world  ".to_string());
        let out = engine
            .render_template(
                "{{ title | trim | capitalize | truncate:8 }}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "Hello...");
    }

    #[test]
    fn unknown_filter_errors_with_clear_message() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "y".to_string());
        let err = engine
            .render_template("{{ x | nonexistent }}", &ctx)
            .unwrap_err();
        match err {
            EngineError::Render(msg) => {
                assert!(
                    msg.contains("Unknown filter: nonexistent"),
                    "{msg}"
                );
            }
            other => panic!("expected Render, got {other:?}"),
        }
    }

    #[test]
    fn whitespace_strip_works_on_block_tags() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("on".to_string(), true);
        let out = engine
            .render_template(
                "a   {{- #if on -}}   x   {{- /if -}}   b",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "axb");
    }

    #[test]
    fn raw_opt_out_still_works_alongside_comments() {
        // Disambiguation: `{{!body}}` (no space after !) is raw
        // substitution, not a comment.
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("body".to_string(), "<b>hi</b>".to_string());
        let out = engine
            .render_template("before {{! note }} after {{!body}}", &ctx)
            .unwrap();
        assert_eq!(out, "before  after <b>hi</b>");
    }

    #[test]
    fn test_render_template_escaping_disabled() {
        let engine = Engine::new("", Duration::from_secs(60))
            .with_html_escape(false);
        let mut context = Context::new();
        context.set("body".to_string(), "<b>hi</b>".to_string());
        let result =
            engine.render_template("{{body}}", &context).unwrap();
        assert_eq!(result, "<b>hi</b>");
    }

    #[test]
    fn test_render_template_whitespace_around_key() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set("name".to_string(), "Alice".to_string());
        let result =
            engine.render_template("Hi {{ name }}", &context).unwrap();
        assert_eq!(result, "Hi Alice");
    }

    #[test]
    fn backslash_escapes_opening_delimiter() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Alice".to_string());
        // Escaped: literal, no lookup.
        let out = engine
            .render_template(
                "literal \\{{name}} and real {{name}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "literal {{name}} and real Alice");
    }

    #[test]
    fn double_backslash_escapes_the_backslash_itself() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Alice".to_string());
        // `\\{{name}}` -> emit one backslash, then substitute.
        let out =
            engine.render_template("path\\\\{{name}}", &ctx).unwrap();
        assert_eq!(out, "path\\Alice");
    }

    #[test]
    fn escape_does_not_leak_into_missing_key_error() {
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx = Context::new();
        // `{{missing}}` without escape still errors.
        let result =
            engine.render_template("\\{{literal}} {{missing}}", &ctx);
        assert!(matches!(result, Err(EngineError::Render(_))));
    }

    #[test]
    fn test_render_page_rejects_path_traversal() {
        let mut engine =
            Engine::new("templates", Duration::from_secs(60));
        let context = Context::new();
        // `a/b` is now allowed; only `..`, absolute paths, and nulls
        // are rejected.
        for bad in ["../etc/passwd", "/etc/passwd", "a\0b", ""] {
            let result = engine.render_page(&context, bad);
            assert!(
                matches!(result, Err(EngineError::InvalidTemplate(_))),
                "expected rejection for {bad:?}"
            );
        }
    }

    #[test]
    fn test_render_page_with_subdirectory() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("blog");
        fs::create_dir_all(&sub_dir).unwrap();
        let template_path = sub_dir.join("post.html");
        fs::write(&template_path, "Post: {{title}}").unwrap();

        let mut engine = Engine::new(
            temp_dir.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut context = Context::new();
        context.set("title".to_string(), "Hello World".to_string());

        let result = engine.render_page(&context, "blog/post").unwrap();
        assert_eq!(result, "Post: Hello World");
    }

    #[test]
    fn test_render_template_with_partial() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let partial_path = temp.path().join("header.html");
        fs::write(&partial_path, "Welcome, {{name}}!").unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut context = Context::new();
        context.set("name".to_string(), "Alice".to_string());

        let template = "Header: {{> header}}";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Header: Welcome, Alice!");
    }

    #[test]
    fn test_render_template_partial_recursion_limit() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let partial_path = temp.path().join("loop.html");
        fs::write(&partial_path, "{{> loop}}").unwrap();

        let engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let context = Context::new();

        let template = "{{> loop}}";
        let result = engine.render_template(template, &context);
        assert!(
            matches!(result, Err(EngineError::Render(msg)) if msg.contains("recursion depth"))
        );
    }

    #[test]
    fn test_render_template_with_filters() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set("name".to_string(), " Alice ".to_string());

        let template = "Hello, {{ name | trim | uppercase }}!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, ALICE!");
    }

    #[test]
    fn test_render_template_truncate_filter() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set(
            "long".to_string(),
            "This is a very long string that should be truncated"
                .to_string(),
        );

        let template = "{{ long | truncate }}";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "This is a very long string ...");
    }

    #[test]
    fn test_render_template_unknown_filter_errors() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set("name".to_string(), "Alice".to_string());

        let template = "{{ name | invalid }}";
        let result = engine.render_template(template, &context);
        assert!(
            matches!(result, Err(EngineError::Render(msg)) if msg.contains("Unknown filter"))
        );
    }

    #[test]
    fn test_render_template_partial_name_empty() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let ctx = Context::new();
        let result = engine.render_template("{{> }}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("Empty partial")
        ));
    }

    #[test]
    fn test_render_template_partial_name_invalid() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let ctx = Context::new();
        let result = engine.render_template("{{> /etc/passwd}}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("invalid template or partial")
        ));
    }

    #[test]
    fn test_render_template_partial_missing_file() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let ctx = Context::new();
        let result = engine.render_template("{{> missing}}", &ctx);
        assert!(matches!(result, Err(EngineError::Io(_))));
    }

    #[test]
    fn test_render_template_unknown_filter() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Alice".to_string());
        let result = engine.render_template("{{name | missing}}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::Render(msg)) if msg.contains("Unknown filter")
        ));
    }

    #[test]
    fn test_render_template_each_unresolved_list() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let ctx = Context::new();
        let result =
            engine.render_template("{{#each missing}}x{{/each}}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::Render(msg)) if msg.contains("#each: unresolved")
        ));
    }

    #[test]
    fn test_render_template_each_not_a_list() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("k".to_string(), 42);
        let result =
            engine.render_template("{{#each k}}x{{/each}}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("expects a list")
        ));
    }

    #[test]
    fn test_extract_block_unclosed_tag() {
        // extract_block calls after_open.find(close) which can return None
        let engine = Engine::new("templates", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("show".to_string(), true);
        // This triggers "Unclosed template tag" inside extract_block
        let result = engine
            .render_template("{{#if show}} {{#if x}} {{/if", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("Unclosed template tag")
        ));
    }

    #[test]
    fn test_extract_block_mismatched_closer() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("show".to_string(), true);
        let result =
            engine.render_template("{{#if show}} {{/each}}", &ctx);
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("Unclosed `{{#if}}` block")
        ));
    }

    #[test]
    fn test_split_else_malformed_tag() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("show".to_string(), true);
        // This triggers "Unclosed `{{#if}}` block" because the parser
        // can't find a clean end for the if-block while skipping the
        // malformed inner tag.
        let result =
            engine.render_template("{{#if show}} {{ {{/if}}", &ctx);
        match &result {
            Err(EngineError::InvalidTemplate(msg)) => {
                assert!(msg.contains("Unclosed `{{#if}}` block"));
            }
            Ok(s) => panic!("Expected error, got success: {}", s),
            other => panic!(
                "Expected InvalidTemplate error, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_validate_path_cases() {
        assert!(validate_path("").is_err());
        assert!(validate_path("/absolute").is_err());
        assert!(validate_path("\\absolute").is_err());
        assert!(validate_path("null\0char").is_err());
        assert!(validate_path("blog/../etc/passwd").is_err());
        assert!(validate_path("blog/post").is_ok());
    }

    #[test]
    fn test_render_template_nested_if() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("outer".to_string(), true);
        ctx.set_value("inner".to_string(), true);
        let out = engine
            .render_template(
                "{{#if outer}}O{{#if inner}}I{{/if}}{{/if}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "OI");
    }

    #[test]
    fn test_render_template_nested_each() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set_value("list".to_string(), vec![vec![1]]);
        let out = engine
            .render_template(
                "{{#each list}}X{{#each this}}{{this}}{{/each}}{{/each}}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "X1");
    }

    #[test]
    fn test_create_template_folder_empty_files_errors() {
        let engine = Engine::new("t", Duration::from_secs(60));
        let result = engine.create_template_folder_with_files(
            Some("http://example.com"),
            &[],
        );
        #[cfg(feature = "remote-templates")]
        assert!(matches!(
            result,
            Err(EngineError::InvalidTemplate(msg)) if msg.contains("must not be empty")
        ));
        #[cfg(not(feature = "remote-templates"))]
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_insert_empty_victim_internal() {
        // We can't easily trigger the None => break branch via the
        // public API since HashMaps with capacity > 0 always have a
        // min_by_key. However, we can use a zero-capacity cache if it
        // were possible. Instead, we'll verify it doesn't crash on
        // empty.
        let mut cache: Cache<String, String> =
            Cache::with_capacity(Duration::from_secs(60), 0);
        let _ = cache.insert("k".to_string(), "v".to_string());
        assert_eq!(cache.len(), 1); // insert still works, cap is a 'soft' hint for eviction
    }

    #[test]
    fn test_render_template_unresolved_tag() {
        let engine = Engine::new("", Duration::from_secs(60));
        let context = Context::new();

        let template = "Hello, {{name}}!";
        let result = engine.render_template(template, &context);
        assert!(matches!(result, Err(EngineError::Render(_))));
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("http://example.com"));
        assert!(is_url("https://example.com"));
        assert!(!is_url("file:///path/to/file"));
        assert!(!is_url("/local/path"));
    }

    #[test]
    fn test_render_page() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let template_path = temp_dir.path().join("template.html");
        fs::write(&template_path, "Hello, {{name}}!").unwrap();

        let mut engine = Engine::new(
            temp_dir.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut context = Context::new();
        context.set("name".to_string(), "World".to_string());

        let result = engine.render_page(&context, "template").unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_clear_cache() {
        let mut engine =
            Engine::new("templates", Duration::from_secs(3600));
        let _ = engine
            .render_cache
            .insert("key1".to_string(), "value1".to_string());
        assert!(!engine.render_cache.is_empty());

        engine.clear_cache();
        assert!(engine.render_cache.is_empty());
    }

    #[test]
    fn test_set_max_cache_size() {
        let mut engine =
            Engine::new("templates", Duration::from_secs(3600));
        let _ = engine
            .render_cache
            .insert("key1".to_string(), "value1".to_string());
        let _ = engine
            .render_cache
            .insert("key2".to_string(), "value2".to_string());
        assert_eq!(engine.render_cache.len(), 2);

        // Capping at 1 doesn't wipe existing entries; subsequent inserts
        // evict the least-recently-used entry to stay within the cap.
        engine.set_max_cache_size(1);
        let _ = engine
            .render_cache
            .insert("key3".to_string(), "value3".to_string());
        assert_eq!(engine.render_cache.len(), 1);
    }

    #[test]
    fn set_max_cache_size_noop_when_under_limit() {
        let mut engine =
            Engine::new("templates", Duration::from_secs(3600));
        let _ = engine
            .render_cache
            .insert("k".to_string(), "v".to_string());
        engine.set_max_cache_size(8);
        assert_eq!(
            engine.render_cache.len(),
            1,
            "no-op branch must preserve cache when len() <= max_size"
        );
    }

    #[test]
    fn render_page_serves_from_cache_on_repeat_call() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let tpl = temp.path().join("page.html");
        fs::write(&tpl, "hi {{name}}").unwrap();

        let mut engine = Engine::new(
            temp.path().to_str().unwrap(),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "alice".to_string());

        let first = engine.render_page(&ctx, "page").unwrap();
        assert_eq!(engine.render_cache.len(), 1);

        // Overwrite the template file; a cached render returns the old
        // output, proving the second call is served from cache rather
        // than re-reading disk.
        fs::write(&tpl, "changed {{name}}").unwrap();
        let second = engine.render_page(&ctx, "page").unwrap();
        assert_eq!(second, first, "second call must hit the cache");
    }

    #[test]
    fn escape_html_covers_every_metacharacter() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut ctx = Context::new();
        ctx.set("x".to_string(), "& < > \" '".to_string());
        let out = engine.render_template("{{x}}", &ctx).unwrap();
        assert_eq!(out, "&amp; &lt; &gt; &quot; &#x27;");
    }

    #[test]
    fn create_template_folder_rejects_none() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        match engine.create_template_folder(None) {
            Err(EngineError::InvalidTemplate(msg)) => {
                assert!(
                    msg.contains("template_path is required"),
                    "{msg}"
                );
            }
            other => panic!("expected InvalidTemplate, got {other:?}"),
        }
    }

    #[test]
    fn create_template_folder_returns_existing_local_path() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        // `create_template_folder` joins against the *current working
        // directory*, so pass a relative segment reachable from the temp
        // dir by chdir'ing.
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        fs::create_dir_all("templates").unwrap();

        let engine = Engine::new(".", Duration::from_secs(60));
        let got = engine.create_template_folder(Some("templates"));

        // Restore CWD before asserting so a failure does not poison
        // subsequent tests in the same process.
        std::env::set_current_dir(&prev).unwrap();
        let path = got.expect("existing local dir must resolve");
        assert!(path.ends_with("templates"), "{path}");
    }

    #[test]
    fn create_template_folder_missing_local_path_is_io_not_found() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        match engine.create_template_folder(Some(
            "this-directory-does-not-exist-on-any-machine",
        )) {
            Err(EngineError::Io(e)) => {
                assert_eq!(e.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("expected Io(NotFound), got {other:?}"),
        }
    }

    #[cfg(not(feature = "remote-templates"))]
    #[test]
    fn create_template_folder_url_without_feature_errors() {
        let engine = Engine::new("templates", Duration::from_secs(60));
        match engine
            .create_template_folder(Some("https://example.com/t/"))
        {
            Err(EngineError::InvalidTemplate(msg)) => {
                assert!(msg.contains("remote-templates"), "{msg}");
            }
            other => {
                panic!("expected InvalidTemplate, got {other:?}")
            }
        }
    }
}
