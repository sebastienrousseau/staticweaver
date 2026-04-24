// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Engine Module
//!
//! This module provides the core functionality for the StaticWeaver templating engine.
//! It includes the `Engine` struct for rendering templates and the `PageOptions` struct
//! for configuring page rendering options.

use crate::cache::Cache;
use crate::context::Context;
use fnv::FnvHashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "remote-templates")]
use std::{fs::File, io::Write, path::PathBuf};

/// Canonical engine error type. Re-exported from `crate::error` to keep a
/// single source of truth; callers can use either `staticweaver::EngineError`
/// or `staticweaver::engine::EngineError` and pattern-match interchangeably.
pub use crate::error::EngineError;

/// Options for rendering a page template.
///
/// This struct contains the options for rendering a page template.
/// These options are used to construct a context `FnvHashMap` that is
/// passed to the `render_template` function.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct PageOptions {
    /// Elements of the page
    pub elements: FnvHashMap<String, String>,
}

impl PageOptions {
    /// Creates a new `PageOptions` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::PageOptions;
    ///
    /// let options = PageOptions::new();
    /// assert!(options.elements.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a page option in the `elements` map.
    ///
    /// # Arguments
    ///
    /// * `key` - The key for the option.
    /// * `value` - The value for the option.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::PageOptions;
    ///
    /// let mut options = PageOptions::new();
    /// options.set("title".to_string(), "My Page".to_string());
    /// assert_eq!(options.get("title"), Some(&"My Page".to_string()));
    /// ```
    pub fn set(&mut self, key: String, value: String) {
        let _ = self.elements.insert(key, value);
    }

    /// Retrieves a page option from the `elements` map.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the option to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the value if the key exists,
    /// or `None` if it doesn't.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::PageOptions;
    ///
    /// let mut options = PageOptions::new();
    /// options.set("title".to_string(), "My Page".to_string());
    /// assert_eq!(options.get("title"), Some(&"My Page".to_string()));
    /// assert_eq!(options.get("nonexistent"), None);
    /// ```
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&String> {
        self.elements.get(key)
    }
}

/// The main template rendering engine.
#[derive(Debug)]
pub struct Engine {
    /// Path to the template directory.
    pub template_path: String,
    /// Cache for rendered templates.
    pub render_cache: Cache<String, String>,
    /// Opening delimiter for template tags.
    pub open_delim: String,
    /// Closing delimiter for template tags.
    pub close_delim: String,
    /// When true, values substituted into templates are HTML-escaped
    /// (`&`, `<`, `>`, `"`, `'`). Prefix a key with `!` to opt out per-tag
    /// (e.g. `{{!content}}` emits the raw value).
    pub escape_html: bool,
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
        }
    }

    /// Toggles HTML escaping for substituted values. Returns `self` for
    /// builder-style chaining. Escaping is on by default; disable it only
    /// when the engine is used to render non-HTML output or when the caller
    /// escapes values themselves.
    #[must_use]
    pub fn with_html_escape(mut self, enable: bool) -> Self {
        self.escape_html = enable;
        self
    }

    /// Renders a page using the specified layout and context, with caching.
    ///
    /// # Arguments
    ///
    /// * `context` - The rendering context, which includes key-value pairs for variable substitution.
    /// * `layout` - The layout file to use for rendering, typically located in the template path.
    ///
    /// # Returns
    ///
    /// A `Result` containing the rendered page as a `String` on success, or an `EngineError` on failure.
    ///
    /// # Errors
    ///
    /// This function can return the following errors:
    /// - `EngineError::Io`: If reading the template file from disk fails.
    /// - `EngineError::Render`: If an error occurs during the rendering process.
    /// - `EngineError::InvalidTemplate`: If the template contains syntax errors.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use staticweaver::Context;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    /// let context = Context::new();
    /// let result = engine.render_page(&context, "default");
    /// ```
    pub fn render_page(
        &mut self,
        context: &Context,
        layout: &str,
    ) -> Result<String, EngineError> {
        // Reject any layout name that could escape the template directory.
        // Callers pass values like `"post"` or `"default"`; slashes, drive
        // letters, null bytes, and `..` segments are never legitimate here.
        if layout.is_empty()
            || layout.contains('/')
            || layout.contains('\\')
            || layout.contains('\0')
            || layout.split(['/', '\\']).any(|seg| seg == "..")
        {
            return Err(EngineError::InvalidTemplate(format!(
                "invalid layout name: {layout:?}"
            )));
        }

        let cache_key = format!("{}:{}", layout, context.hash());

        // Return cached result if available
        if let Some(cached) = self.render_cache.get(&cache_key) {
            return Ok(cached.to_string());
        }

        // Attempt to read the layout template from the file system
        let template_path = Path::new(&self.template_path)
            .join(format!("{layout}.html"));
        let template_content = fs::read_to_string(&template_path)?;

        // Render the template with the provided context
        let rendered =
            self.render_template(&template_content, context)?;

        // Cache the rendered result for future use
        let _ = self.render_cache.insert(cache_key, rendered.clone());

        Ok(rendered)
    }

    /// Renders a template string with the given context and custom delimiters.
    ///
    /// # Arguments
    ///
    /// * `template` - The template string containing the tags to be replaced.
    /// * `context` - A `Context` containing the key-value pairs to use for substitution.
    ///
    /// # Returns
    ///
    /// A `Result` containing the rendered string or an `EngineError` if an error occurs.
    ///
    /// # Errors
    ///
    /// * `EngineError::InvalidTemplate` - If the template contains unclosed tags or is empty.
    /// * `EngineError::Render` - If a template tag cannot be resolved from the context.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use staticweaver::Context;
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(3600));
    /// let mut context = Context::new();
    /// context.set("greeting".to_string(), "Hello".to_string());
    /// context.set("name".to_string(), "Alice".to_string());
    ///
    /// let template = "{{greeting}}, {{name}}!";
    /// let result = engine.render_template(template, &context).unwrap();
    /// assert_eq!(result, "Hello, Alice!");
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

        let open = self.open_delim.as_str();
        let close = self.close_delim.as_str();
        let mut output = String::with_capacity(template.len());
        let mut rest = template;

        while let Some(start) = rest.find(open) {
            output.push_str(&rest[..start]);
            let after_open = &rest[start + open.len()..];

            let end = after_open.find(close).ok_or_else(|| {
                EngineError::InvalidTemplate(
                    "Unclosed template tag".to_string(),
                )
            })?;

            let key_raw = &after_open[..end];

            // Reject an opening delimiter inside the key — catches both
            // nested tags and malformed input like `{{foo{{bar}}}}`.
            if key_raw.contains(open) {
                return Err(EngineError::InvalidTemplate(
                    "Nested delimiters are not allowed".to_string(),
                ));
            }

            let key_trimmed = key_raw.trim();
            let (lookup, raw) = match key_trimmed.strip_prefix('!') {
                Some(stripped) => (stripped.trim_start(), true),
                None => (key_trimmed, false),
            };

            let value = context.get(lookup).ok_or_else(|| {
                EngineError::Render(format!(
                    "Unresolved template tag: {lookup}"
                ))
            })?;

            if raw || !self.escape_html {
                output.push_str(value);
            } else {
                escape_html_into(value, &mut output);
            }

            rest = &after_open[end + close.len()..];
        }

        output.push_str(rest);
        Ok(output)
    }

    /// Sets custom delimiters for the template tags.
    ///
    /// # Arguments
    ///
    /// * `open` - The string to use as the opening delimiter (e.g., `<<`).
    /// * `close` - The string to use as the closing delimiter (e.g., `>>`).
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    /// engine.set_delimiters("<<", ">>");
    /// ```
    pub fn set_delimiters(&mut self, open: &str, close: &str) {
        self.open_delim = open.to_string();
        self.close_delim = close.to_string();
    }

    /// Resolves a template folder, either from a local directory or — when
    /// the `remote-templates` feature is enabled — from an HTTP/S URL.
    ///
    /// # Arguments
    ///
    /// * `template_path` - An optional path or URL. `None` is an error; pass
    ///   an explicit directory or URL. (Earlier versions silently fetched
    ///   from a hardcoded third-party URL — that default is gone.)
    ///
    /// # Errors
    ///
    /// - `EngineError::Io`: the directory does not exist.
    /// - `EngineError::InvalidTemplate`: `template_path` is `None`, or a URL
    ///   was supplied without the `remote-templates` feature.
    /// - `EngineError::Reqwest` / `EngineError::Render`: when
    ///   `remote-templates` is enabled and the fetch fails.
    pub fn create_template_folder(
        &self,
        template_path: Option<&str>,
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
                let dir = Self::download_files_from_url(path)?;
                return dir.to_str().map(str::to_string).ok_or_else(
                    || {
                        EngineError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Invalid UTF-8 sequence in template path",
                        ))
                    },
                );
            }
            #[cfg(not(feature = "remote-templates"))]
            return Err(EngineError::InvalidTemplate(
                "remote template URLs require the `remote-templates` feature"
                    .to_string(),
            ));
        }

        let current_dir = std::env::current_dir()?;
        let local_path = current_dir.join(path);
        if local_path.exists() && local_path.is_dir() {
            local_path.to_str().map(str::to_string).ok_or_else(|| {
                EngineError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid UTF-8 sequence in template path",
                ))
            })
        } else {
            Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Template directory not found: {path}"),
            )))
        }
    }

    /// Downloads the default set of template files from `url` into a fresh
    /// temporary directory and returns its path. The temp directory is
    /// owned by the caller via `TempDir` and will be cleaned up on drop.
    #[cfg(feature = "remote-templates")]
    fn download_files_from_url(
        url: &str,
    ) -> Result<PathBuf, EngineError> {
        let dir = tempfile::tempdir()?;
        // `keep` (stable replacement for the deprecated `into_path`) returns
        // a PathBuf and suppresses cleanup; we accept that here because the
        // caller treats the downloaded template dir as long-lived.
        let template_dir_path = dir.keep();

        let files = [
            "contact.html",
            "index.html",
            "page.html",
            "post.html",
            "main.js",
            "sw.js",
        ];

        for file in &files {
            Self::download_file(url, file, &template_dir_path)?;
        }

        Ok(template_dir_path)
    }

    /// Downloads a single file from `url/file` into `dir`, with a 10s
    /// timeout, an HTTP status check, and a 1 MiB body cap so a hostile or
    /// misconfigured server cannot exhaust memory.
    #[cfg(feature = "remote-templates")]
    fn download_file(
        url: &str,
        file: &str,
        dir: &Path,
    ) -> Result<(), EngineError> {
        /// Per-file body cap. Template assets are HTML/JS/CSS; a megabyte is
        /// far above any realistic payload.
        const MAX_BYTES: usize = 1 * 1024 * 1024;

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

    /// Clears all cached rendered templates.
    ///
    /// This method removes all entries from the cache, freeing up memory.
    /// After calling this, subsequent render requests will not retrieve
    /// any cached results and will regenerate the templates.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    /// // Cache some templates...
    ///
    /// // Clear the cache
    /// engine.clear_cache();
    /// ```
    pub fn clear_cache(&mut self) {
        self.render_cache.clear();
    }

    /// Sets a maximum size for the render cache and clears the cache if it exceeds the specified limit.
    ///
    /// This method allows you to define a maximum number of entries that can be stored in the render cache.
    /// If the cache size exceeds this limit, the cache will be cleared to prevent unbounded memory usage.
    ///
    /// # Arguments
    ///
    /// * `max_size` - The maximum number of cache entries allowed before the cache is cleared.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    ///
    /// // Set a maximum cache size of 100 entries
    /// engine.set_max_cache_size(100);
    /// ```
    pub fn set_max_cache_size(&mut self, max_size: usize) {
        if self.render_cache.len() > max_size {
            self.clear_cache();
        }
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

/// Appends `s` to `out`, replacing the five HTML metacharacters with their
/// named/numeric entities. Single-quote uses the numeric `&#x27;` form so
/// the output stays valid inside both HTML and XML attributes.
fn escape_html_into(s: &str, out: &mut String) {
    out.reserve(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
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
    fn test_render_template_raw_opt_out() {
        let engine = Engine::new("", Duration::from_secs(60));
        let mut context = Context::new();
        context.set("body".to_string(), "<b>hi</b>".to_string());
        let result =
            engine.render_template("{{!body}}", &context).unwrap();
        assert_eq!(result, "<b>hi</b>");
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
    fn test_render_page_rejects_path_traversal() {
        let mut engine =
            Engine::new("templates", Duration::from_secs(60));
        let context = Context::new();
        for bad in ["../etc/passwd", "a/b", "a\\b", ""] {
            let result = engine.render_page(&context, bad);
            assert!(
                matches!(result, Err(EngineError::InvalidTemplate(_))),
                "expected rejection for {bad:?}"
            );
        }
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
    fn test_page_options() {
        let mut options = PageOptions::new();
        options.set("title".to_string(), "My Page".to_string());
        assert_eq!(options.get("title"), Some(&"My Page".to_string()));
        assert_eq!(options.get("non_existent"), None);
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

        engine.set_max_cache_size(1);
        assert!(engine.render_cache.is_empty());
    }
}
