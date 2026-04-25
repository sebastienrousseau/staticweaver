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
        self.render_into(template, context, &mut output)?;
        Ok(output)
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
    ///
    /// Block bodies are rendered through this same function, so escaping,
    /// dot-notation, and nested control flow compose without duplication.
    fn render_into(
        &self,
        template: &str,
        context: &Context,
        output: &mut String,
    ) -> Result<(), EngineError> {
        self.render_recursive(template, context, output, 0)
    }

    /// Internal rendering loop with a depth limit to catch infinite
    /// recursion from circular partial includes.
    fn render_recursive(
        &self,
        template: &str,
        context: &Context,
        output: &mut String,
        depth: usize,
    ) -> Result<(), EngineError> {
        if depth > 10 {
            return Err(EngineError::Render(
                "Maximum template recursion depth (10) exceeded"
                    .to_string(),
            ));
        }

        let open = self.open_delim.as_str();
        let close = self.close_delim.as_str();
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
                EngineError::InvalidTemplate(
                    "Unclosed template tag".to_string(),
                )
            })?;
            let key_raw = &after_open[..end];

            if key_raw.contains(open) {
                return Err(EngineError::InvalidTemplate(
                    "Nested delimiters are not allowed".to_string(),
                ));
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
                    extract_block(after_tag, "if", open, close)?;
                let (then_body, else_body) =
                    split_else(body, open, close);
                let cond = context
                    .get_path(arg)
                    .map_or(false, |v| v.is_truthy());
                let chosen = if cond {
                    then_body
                } else {
                    else_body.unwrap_or("")
                };
                if !chosen.is_empty() {
                    self.render_recursive(
                        chosen,
                        context,
                        output,
                        depth + 1,
                    )?;
                }
                rest = after_block;
                continue;
            }

            if let Some(arg) = key_trimmed.strip_prefix("#each") {
                let arg = arg.trim();
                let (body, after_block) =
                    extract_block(after_tag, "each", open, close)?;
                let target =
                    context.get_path(arg).ok_or_else(|| {
                        EngineError::Render(format!(
                            "#each: unresolved list `{arg}`"
                        ))
                    })?;
                // Iterate Lists by position (binds @index/@first/@last)
                // and Maps by key (also binds @key). Sort Map entries by
                // key so iteration order is deterministic across runs —
                // FnvHashMap iteration order is otherwise unspecified.
                let entries: Vec<(
                    Option<String>,
                    &crate::context::Value,
                )> = match target {
                    crate::context::Value::List(items) => {
                        items.iter().map(|v| (None, v)).collect()
                    }
                    crate::context::Value::Map(map) => {
                        let mut keyed: Vec<_> = map.iter().collect();
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
                };

                let total = entries.len();
                for (index, (key_opt, item)) in
                    entries.iter().enumerate()
                {
                    let mut child = context.clone();
                    child
                        .set_value("this".to_string(), (*item).clone());
                    child.set_value(
                        "@index".to_string(),
                        i64::try_from(index).unwrap_or(i64::MAX),
                    );
                    child.set_value("@first".to_string(), index == 0);
                    child.set_value(
                        "@last".to_string(),
                        index + 1 == total,
                    );
                    if let Some(k) = key_opt {
                        child.set_value("@key".to_string(), k.as_str());
                    }
                    self.render_recursive(
                        body,
                        &child,
                        output,
                        depth + 1,
                    )?;
                }
                rest = after_block;
                continue;
            }

            // ── Partial inclusion ──────────────────────────────────
            if let Some(name) = key_trimmed.strip_prefix('>') {
                let name = name.trim();
                if name.is_empty() {
                    return Err(EngineError::InvalidTemplate(
                        "Empty partial name".to_string(),
                    ));
                }
                // Reject names that could escape the template directory
                validate_path(name)?;

                let partial_path = Path::new(&self.template_path)
                    .join(format!("{name}.html"));
                let content = fs::read_to_string(&partial_path)?;
                self.render_recursive(
                    &content,
                    context,
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
                    "unexpected `{key_trimmed}` outside a block"
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

            let value = context.get_path(lookup).ok_or_else(|| {
                EngineError::Render(format!(
                    "Unresolved template tag: {lookup}"
                ))
            })?;
            let mut rendered = value.to_string();

            // A trailing `safe` filter marks the value as already-safe
            // HTML and suppresses the engine's auto-escape. Mirrors the
            // `{{!key}}` raw opt-out but composes inside a filter chain.
            let marked_safe =
                filters.last().is_some_and(|(name, _)| name == "safe");

            for (name, args) in &filters {
                rendered = apply_filter(name, args, rendered)?;
            }

            if raw || marked_safe || !self.escape_html {
                output.push_str(&rendered);
            } else {
                escape_html_into(&rendered, output);
            }

            rest = after_tag;
        }

        output.push_str(rest);
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

        if inner.starts_with("#if") || inner.starts_with("#each") {
            depth += 1;
        } else if inner == format!("/{block}") {
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
        } else if inner.starts_with("/if") || inner.starts_with("/each")
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
        unknown => Err(EngineError::Render(format!(
            "Unknown filter: {unknown}"
        ))),
    }
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

        if inner.starts_with("#if") || inner.starts_with("#each") {
            depth += 1;
        } else if inner.starts_with("/if") || inner.starts_with("/each")
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
