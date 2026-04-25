// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Engine Module
//!
//! This module provides the core functionality for the StaticWeaver templating engine.
//! It contains the `Engine` struct for rendering templates against a `Context`.

use crate::cache::Cache;
use crate::context::Context;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "remote-templates")]
use std::{fs::File, io::Write, path::PathBuf};

/// Canonical engine error type. Re-exported from `crate::error` to keep a
/// single source of truth; callers can use either `staticweaver::EngineError`
/// or `staticweaver::engine::EngineError` and pattern-match interchangeably.
pub use crate::error::EngineError;

/// Filenames fetched by default when `Engine::create_template_folder` is
/// called with an HTTP/S URL and no explicit file list. Matches the
/// historical six-file set for backwards compatibility; callers who need
/// a different layout should use
/// [`Engine::create_template_folder_with_files`].
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
            // Every pair of backslashes collapses to a single literal.
            for _ in 0..bs / 2 {
                output.push('\\');
            }
            if bs % 2 == 1 {
                // Odd -> emit the delimiter literally and continue.
                output.push_str(open);
                rest = &rest[start + open.len()..];
                continue;
            }

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

            // Resolve via dot-notation path so nested values are
            // reachable (e.g. `{{user.name}}`). Display formats every
            // primitive variant inline; non-leaf List/Map render as
            // empty strings, which is the correct behaviour for direct
            // substitution (control-flow blocks consume those types).
            let value = context.get_path(lookup).ok_or_else(|| {
                EngineError::Render(format!(
                    "Unresolved template tag: {lookup}"
                ))
            })?;
            let rendered = value.to_string();

            if raw || !self.escape_html {
                output.push_str(&rendered);
            } else {
                escape_html_into(&rendered, &mut output);
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
    /// let _ = engine.create_template_folder_with_files(None, &["index.html"]);
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
                        EngineError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
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
    /// Caps the render cache at `max_size` entries. Subsequent inserts at
    /// or above the cap evict the least-recently-used entry automatically
    /// — no more wholesale cache wipes.
    ///
    /// # Arguments
    ///
    /// * `max_size` - The maximum number of cache entries allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    /// engine.set_max_cache_size(100);
    /// ```
    pub fn set_max_cache_size(&mut self, max_size: usize) {
        self.render_cache.set_capacity(max_size);
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
///
/// Byte-level scan: iterate over `s.as_bytes()`, flush clean runs via
/// `push_str`, substitute only the five ASCII metacharacters. Valid UTF-8
/// guarantees any byte `<= 0x7F` sits on a char boundary, so slicing at
/// those positions is always valid UTF-8 — no `unsafe` required.
fn escape_html_into(s: &str, out: &mut String) {
    out.reserve(s.len());
    let bytes = s.as_bytes();
    let mut last = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let entity: &str = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#x27;",
            _ => continue,
        };
        out.push_str(&s[last..i]);
        out.push_str(entity);
        last = i + 1;
    }
    out.push_str(&s[last..]);
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
                assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
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
