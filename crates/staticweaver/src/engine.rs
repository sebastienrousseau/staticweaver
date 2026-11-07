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
use staticweaver_core::path::{resolve_template_path, validate_name};
use staticweaver_core::Renderer;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
#[cfg(feature = "remote-templates")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Filter, test and loader extension points. Defined in
/// `staticweaver-core` and re-exported here at their historical paths.
pub use staticweaver_core::{FilterFn, TemplateLoader, TestFn};

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
        let path =
            resolve_template_path(&self.root, &format!("{name}.html"))?;
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

#[cfg(feature = "remote-templates")]
use std::fs::File;

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
    /// Wrapped in `Mutex` (issue #36, v0.0.4) so `Engine` is `Send + Sync`
    /// and `render_page` can take `&self` — making the engine sharable
    /// across threads / async tasks via `Arc<Engine>` without the
    /// `Arc<Mutex<Engine>>` envelope users previously had to write.
    /// The lock is held only across the cache lookup or insert; the
    /// expensive render work happens outside it.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::Engine;
    /// use std::time::Duration;
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// assert_eq!(engine.render_cache.lock().unwrap().len(), 0);
    /// ```
    pub render_cache: std::sync::Mutex<Cache<String, String>>,
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
    /// When `true`, an unresolved `{{key}}` substitution emits an
    /// empty string instead of aborting the render with
    /// `EngineError::Render("Unresolved template tag: <key>")`.
    /// Defaults to `false` (strict). Mirrors Jinja2's "undefined"
    /// mode and lets sites whose page templates reference variables
    /// that not every page supplies build without per-page shims.
    /// Toggle via [`Engine::with_lax_undefined`]. Issue #28.
    pub lax_undefined: bool,
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
            .field("lax_undefined", &self.lax_undefined)
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
            render_cache: std::sync::Mutex::new(Cache::new(cache_ttl)),
            open_delim: "{{".to_string(),
            close_delim: "}}".to_string(),
            escape_html: true,
            custom_filters: HashMap::new(),
            custom_tests: HashMap::new(),
            loader: Arc::new(FsLoader::new(PathBuf::from(
                template_path,
            ))),
            autoescape_extensions: Vec::new(),
            lax_undefined: false,
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
            render_cache: std::sync::Mutex::new(Cache::new(cache_ttl)),
            open_delim: "{{".to_string(),
            close_delim: "}}".to_string(),
            escape_html: true,
            custom_filters: HashMap::new(),
            custom_tests: HashMap::new(),
            loader,
            autoescape_extensions: Vec::new(),
            lax_undefined: false,
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

    /// Sets whether unresolved `{{key}}` substitutions abort the
    /// render (strict, default) or emit an empty string (lax).
    ///
    /// Lax mode mirrors Jinja2's "undefined" behaviour and lets
    /// sites whose page templates reference variables that not every
    /// page supplies build without per-page shims. Issue #28.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    /// let mut engine = Engine::new("", Duration::from_secs(60))
    ///     .with_lax_undefined(true);
    /// let ctx = Context::new();
    /// assert_eq!(
    ///     engine.render_template("hello {{name}}", &ctx).unwrap(),
    ///     "hello ",
    /// );
    /// ```
    #[must_use]
    pub fn with_lax_undefined(mut self, lax: bool) -> Self {
        self.lax_undefined = lax;
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "debug",
            name = "staticweaver.render_page",
            skip(self, context),
            fields(layout = %layout, context.len = context.len()),
        )
    )]
    pub fn render_page(
        &self,
        context: &Context,
        layout: &str,
    ) -> Result<String, EngineError> {
        // Reject any layout name that could escape the template directory.
        // Callers pass values like "post", "default", or "blog/post".
        validate_name(layout)?;

        let cache_key = format!("{}:{}", layout, context.hash());

        // Return cached result if available. The Mutex (issue #36)
        // is held only across the lookup — the expensive render work
        // below happens outside it, so concurrent render_page calls
        // for *different* keys parallelise.
        if let Some(cached) = self
            .render_cache
            .lock()
            .expect("cache poisoned")
            .get(&cache_key)
        {
            return Ok(cached.to_string());
        }

        // Load the layout body via the configured TemplateLoader
        // (defaults to FsLoader rooted at `self.template_path`).
        let template_content = self.loader.load(layout)?;

        // Render the template with the provided context, under the
        // layout's per-extension escape policy.
        let output = self
            .renderer(self.effective_escape(layout))
            .render(&template_content, context)?;

        // Cache the rendered result for future use. Lock-and-insert
        // is a separate critical section from the lookup above — a
        // concurrent insert for the same key races but is harmless
        // (last-writer wins; both renders produce the same bytes
        // because the inputs are identical).
        let _ = self
            .render_cache
            .lock()
            .expect("cache poisoned")
            .insert(cache_key, output.clone());

        Ok(output)
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "debug",
            name = "staticweaver.render_template",
            skip(self, context),
            fields(template.bytes = template.len()),
        )
    )]
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

        self.renderer(self.escape_html).render(template, context)
    }

    /// Per-extension auto-escape policy: if the user opted in via
    /// `autoescape_on(&[".html", …])`, the layout's own extension
    /// decides whether substitutions get escaped, not the global
    /// `escape_html` flag. Computed per call and passed down, with no
    /// mutation of `self.escape_html` (which would make render_page
    /// require `&mut self` and race across concurrent renders).
    fn effective_escape(&self, layout: &str) -> bool {
        if self.autoescape_extensions.is_empty() {
            self.escape_html
        } else {
            self.autoescape_extensions
                .iter()
                .any(|ext| layout.ends_with(ext.as_str()))
        }
    }

    /// Borrows this engine's configuration as a core
    /// [`Renderer`](staticweaver_core::Renderer).
    fn renderer(&self, escape_html: bool) -> Renderer<'_> {
        Renderer::new(&*self.loader)
            .delimiters(&self.open_delim, &self.close_delim)
            .escape_html(escape_html)
            .lax_undefined(self.lax_undefined)
            .filters(&self.custom_filters)
            .tests(&self.custom_tests)
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
        &self,
        context: &Context,
        layout: &str,
        writer: &mut W,
    ) -> Result<(), EngineError> {
        let rendered = self.render_page(context, layout)?;
        writer.write_all(rendered.as_bytes())?;
        Ok(())
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
    pub fn set_max_cache_size(&self, size: usize) {
        self.render_cache
            .lock()
            .expect("cache poisoned")
            .set_capacity(size);
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
    pub fn clear_cache(&self) {
        self.render_cache.lock().expect("cache poisoned").clear();
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
                crate::transport::require_permitted(path)?;
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

        let client = reqwest::blocking::Client::builder()
            .redirect(crate::transport::redirect_policy())
            .build()?;
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

// ── Async render surface (issue #37 + #38) ──────────────────────────
//
// Hidden behind `#[cfg(feature = "async")]` so default builds stay
// sync-only. The render machinery itself is CPU-bound and remains
// sync; the async surface only wraps the loader IO so callers in
// async runtimes don't have to spawn_blocking just to fetch a
// template body.

#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
impl Engine {
    /// Async counterpart to [`Engine::render_template`]: loads the
    /// named template via `loader` (asynchronously), then renders
    /// it synchronously against `context`. Issue #37.
    ///
    /// The render itself is CPU-bound and stays sync — the async-ness
    /// comes from the loader IO. If you already have the template
    /// body in memory, prefer `render_template` directly.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires the `async-tokio` feature + a tokio runtime.
    /// use staticweaver::loader_async::MemoryAsyncLoader;
    /// use staticweaver::{Context, Engine};
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// # async fn run() -> Result<(), staticweaver::EngineError> {
    /// let mut store = HashMap::new();
    /// let _ = store.insert("hi".to_string(), "Hello {{name}}!".to_string());
    /// let loader = MemoryAsyncLoader::new(store);
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set("name".to_string(), "Ada".to_string());
    /// let out = engine.render_template_async(&loader, "hi", &ctx).await?;
    /// assert_eq!(out, "Hello Ada!");
    /// # Ok(()) }
    /// ```
    pub async fn render_template_async<L>(
        &self,
        loader: &L,
        name: &str,
        context: &Context,
    ) -> Result<String, EngineError>
    where
        L: staticweaver_async::AsyncTemplateLoader,
    {
        validate_name(name)?;
        let body = loader.load(name).await?;
        self.render_template(&body, context)
    }

    /// Async counterpart to [`Engine::render_page`]: loads the layout
    /// via `loader`, applies the per-extension auto-escape policy,
    /// renders, and memoises the result in the same Mutex-protected
    /// `render_cache` the sync path uses. Issue #37.
    ///
    /// Two concurrent async tasks calling this with the same key
    /// race the insert harmlessly — both produce identical bytes
    /// because the inputs are identical.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires the `async-tokio` feature + a tokio runtime.
    /// use staticweaver::loader_async::MemoryAsyncLoader;
    /// use staticweaver::{Context, Engine};
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// # async fn run() -> Result<(), staticweaver::EngineError> {
    /// let mut store = HashMap::new();
    /// let _ = store.insert("page".to_string(), "v={{v}}".to_string());
    /// let loader = MemoryAsyncLoader::new(store);
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set("v".to_string(), "ok".to_string());
    /// let out = engine.render_page_async(&loader, &ctx, "page").await?;
    /// assert_eq!(out, "v=ok");
    /// # Ok(()) }
    /// ```
    pub async fn render_page_async<L>(
        &self,
        loader: &L,
        context: &Context,
        layout: &str,
    ) -> Result<String, EngineError>
    where
        L: staticweaver_async::AsyncTemplateLoader,
    {
        validate_name(layout)?;

        let cache_key = format!("{}:{}", layout, context.hash());
        if let Some(cached) = self
            .render_cache
            .lock()
            .expect("cache poisoned")
            .get(&cache_key)
        {
            return Ok(cached.to_string());
        }

        let template_content = loader.load(layout).await?;
        let output = self
            .renderer(self.effective_escape(layout))
            .render(&template_content, context)?;

        let _ = self
            .render_cache
            .lock()
            .expect("cache poisoned")
            .insert(cache_key, output.clone());

        Ok(output)
    }

    /// Async streaming sink — issue #38. Mirror of
    /// [`Engine::render_to`] for `AsyncWrite`-flavoured destinations
    /// (`tokio::fs::File`, `tokio::net::TcpStream`, an Axum body
    /// channel). The render itself stays sync; the bytes flush
    /// through the async writer.
    ///
    /// Requires the `async-tokio` feature for the `AsyncWriteExt`
    /// extension trait.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires the `async-tokio` feature + a tokio runtime.
    /// use staticweaver::{Context, Engine};
    /// use std::time::Duration;
    ///
    /// # async fn run() -> Result<(), staticweaver::EngineError> {
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set("who".to_string(), "world".to_string());
    /// let mut sink: Vec<u8> = Vec::new();
    /// engine.render_to_async("hi {{who}}", &ctx, &mut sink).await?;
    /// assert_eq!(&sink, b"hi world");
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "async-tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-tokio")))]
    pub async fn render_to_async<W>(
        &self,
        template: &str,
        context: &Context,
        writer: &mut W,
    ) -> Result<(), EngineError>
    where
        W: tokio::io::AsyncWrite + Unpin + Send,
    {
        use tokio::io::AsyncWriteExt;
        let body = self.render_template(template, context)?;
        writer.write_all(body.as_bytes()).await?;
        Ok(())
    }

    /// Page-flavoured async streaming sink — issue #38. Combines
    /// async load + sync render + async write into one call. Errors
    /// surface as `EngineError`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // Requires the `async-tokio` feature + a tokio runtime.
    /// use staticweaver::loader_async::MemoryAsyncLoader;
    /// use staticweaver::{Context, Engine};
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// # async fn run() -> Result<(), staticweaver::EngineError> {
    /// let mut store = HashMap::new();
    /// let _ = store.insert("p".to_string(), "[{{a}}]".to_string());
    /// let loader = MemoryAsyncLoader::new(store);
    /// let engine = Engine::new("", Duration::from_secs(60));
    /// let mut ctx = Context::new();
    /// ctx.set("a".to_string(), "ok".to_string());
    /// let mut sink: Vec<u8> = Vec::new();
    /// engine.render_page_to_async(&loader, &ctx, "p", &mut sink).await?;
    /// assert_eq!(&sink, b"[ok]");
    /// # Ok(()) }
    /// ```
    #[cfg(feature = "async-tokio")]
    #[cfg_attr(docsrs, doc(cfg(feature = "async-tokio")))]
    pub async fn render_page_to_async<L, W>(
        &self,
        loader: &L,
        context: &Context,
        layout: &str,
        writer: &mut W,
    ) -> Result<(), EngineError>
    where
        L: staticweaver_async::AsyncTemplateLoader,
        W: tokio::io::AsyncWrite + Unpin + Send,
    {
        use tokio::io::AsyncWriteExt;
        let body =
            self.render_page_async(loader, context, layout).await?;
        writer.write_all(body.as_bytes()).await?;
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

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
