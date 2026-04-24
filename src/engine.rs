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
use reqwest;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;
use thiserror::Error;

/// Error types specific to the engine operations.
#[derive(Debug, Error)]
pub enum EngineError {
    /// I/O related errors.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Network request related errors.
    #[error("Request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Template rendering errors.
    #[error("Render error: {0}")]
    Render(String),

    /// Invalid template syntax errors.
    #[error("Invalid template: {0}")]
    InvalidTemplate(String),
}

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
            || layout
                .split(|c| c == '/' || c == '\\')
                .any(|seg| seg == "..")
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

    /// Creates or uses an existing template folder.
    ///
    /// # Arguments
    ///
    /// * `template_path` - An optional path to the template folder. It can be a local path or a URL.
    ///
    /// # Returns
    ///
    /// A `Result` containing the template folder path as a `String` on success, or an `EngineError` on failure.
    ///
    /// # Errors
    ///
    /// This function can return the following errors:
    /// - `EngineError::Io`: If there is an issue with file operations.
    /// - `EngineError::Reqwest`: If there is an issue downloading files from a URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let engine = Engine::new("templates", Duration::from_secs(3600));
    /// let result = engine.create_template_folder(Some("custom_templates"));
    /// ```
    pub fn create_template_folder(
        &self,
        template_path: Option<&str>,
    ) -> Result<String, EngineError> {
        let current_dir = std::env::current_dir()?;

        let template_dir_path = match template_path {
            Some(path) if is_url(path) => {
                // Download template files from the URL
                Self::download_files_from_url(path)?
            }
            Some(path) => {
                // Use the local directory if it exists
                let local_path = current_dir.join(path);
                if local_path.exists() && local_path.is_dir() {
                    local_path
                } else {
                    // Return an I/O error if the directory is not found
                    return Err(EngineError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "Template directory not found: {}",
                            path
                        ),
                    )));
                }
            }
            None => {
                // Default to downloading template files from the default URL
                let default_url = "https://raw.githubusercontent.com/sebastienrousseau/shokunin/main/template/";
                Self::download_files_from_url(default_url)?
            }
        };

        // Ensure the template path is valid UTF-8
        Ok(template_dir_path
            .to_str()
            .ok_or_else(|| {
                EngineError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid UTF-8 sequence in template path",
                ))
            })?
            .to_string())
    }

    /// Helper function to download files from a URL and save to a directory.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to download files from.
    ///
    /// # Returns
    ///
    /// A `Result` containing the path to the directory or an `EngineError`.
    fn download_files_from_url(
        url: &str,
    ) -> Result<PathBuf, EngineError> {
        let template_dir_path = tempdir()?.into_path();

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

    /// Downloads a single file from a URL to the given directory.
    ///
    /// # Arguments
    ///
    /// * `url` - The base URL.
    /// * `file` - The file to download.
    /// * `dir` - The directory to save the file.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or an `EngineError`.
    fn download_file(
        url: &str,
        file: &str,
        dir: &Path,
    ) -> Result<(), EngineError> {
        let file_url = format!("{}/{}", url, file);
        let file_path = dir.join(file);

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(&file_url)
            .timeout(Duration::from_secs(10)) // Set a timeout
            .send()?;

        // Check if the response status is not a success (200-299)
        if !response.status().is_success() {
            return Err(EngineError::Render(format!(
                "Failed to download {}: HTTP {}",
                file,
                response.status()
            )));
        }

        // Proceed with file saving if the response is successful
        let mut file = File::create(&file_path)?;
        let content = response.text()?;
        file.write_all(content.as_bytes())?;

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
