// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

// src/engine.rs

//! # Template Rendering Engine
//!
//! This module provides a template rendering engine with caching capabilities.
//! It supports rendering templates from files or strings, with context-based
//! variable substitution.

use crate::cache::Cache;
use crate::Context;
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

/// Error types specific to the engine operations.
#[derive(Debug, thiserror::Error)]
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
/// These options are used to construct a context `HashMap` that is
/// passed to the `render_template` function.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct PageOptions {
    /// Elements of the page
    pub elements: HashMap<String, String>,
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
    pub fn new() -> PageOptions {
        PageOptions {
            elements: HashMap::new(),
        }
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
#[derive(Debug, Default)]
pub struct Engine {
    /// Path to the template directory.
    pub template_path: String,
    /// Cache for rendered templates.
    pub render_cache: Cache<String, String>,
    /// Opening delimiter for template tags.
    pub open_delim: String,
    /// Closing delimiter for template tags.
    pub close_delim: String,
}

impl Engine {
    /// Creates a new `Engine` instance.
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
        }
    }

    /// Renders a page using the specified layout and context, with caching.
    ///
    /// This method retrieves the template for the specified layout, applies the provided context for
    /// rendering, and caches the result for future requests. If the template has been previously cached,
    /// the cached result is returned to improve performance.
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
    /// - `EngineError::Io`: If reading the template file from disk fails (e.g., file not found).
    /// - `EngineError::Render`: If an error occurs during the rendering process (e.g., unresolved template tags).
    /// - `EngineError::InvalidTemplate`: If the template contains syntax errors (e.g., unclosed tags).
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
        let cache_key = format!("{}:{}", layout, context.hash());

        // Return cached result if available
        if let Some(cached) = self.render_cache.get(&cache_key) {
            return Ok(cached.to_string());
        }

        // Attempt to read the layout template from the file system
        let template_path = Path::new(&self.template_path)
            .join(format!("{}.html", layout));
        let template_content = fs::read_to_string(&template_path)?;

        // Render the template with the provided context
        let rendered =
            self.render_template(&template_content, &context.elements)?;

        // Cache the rendered result for future use
        let _ = self.render_cache.insert(cache_key, rendered.clone());

        Ok(rendered)
    }

    /// Renders a template string with the given context and custom delimiters.
    ///
    /// This method takes a template string and a context (key-value pairs) and replaces
    /// any template tags found within the string with the corresponding values from the
    /// context. The template tags are defined by the opening and closing delimiters,
    /// which can be customized using the `set_delimiters` method. If a template tag
    /// cannot be resolved (i.e., the key does not exist in the context), an error is returned.
    ///
    /// The method also ensures that all template tags are properly closed, otherwise,
    /// it returns an error for invalid template syntax.
    ///
    /// # Arguments
    ///
    /// * `template` - The template string containing the tags to be replaced.
    /// * `context` - A `HashMap` containing the key-value pairs to use for substitution.
    ///
    /// # Returns
    ///
    /// A `Result` containing the rendered string or an `EngineError` if an error occurs
    /// (e.g., unresolved template tag, invalid template syntax).
    ///
    /// # Errors
    ///
    /// * `EngineError::InvalidTemplate` - If the template contains unclosed tags or is empty.
    /// * `EngineError::Render` - If a template tag cannot be resolved from the context.
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::collections::HashMap;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    /// engine.set_delimiters("<<", ">>");
    ///
    /// let mut context = HashMap::new();
    /// context.insert("greeting".to_string(), "Hello".to_string());
    /// context.insert("name".to_string(), "Alice".to_string());
    ///
    /// let template = "<<greeting>>, <<name>>!";
    /// let result = engine.render_template(template, &context).unwrap();
    /// assert_eq!(result, "Hello, Alice!");
    /// ```
    ///
    /// # Panics
    ///
    /// This method will panic if the delimiters are not properly set or if the template
    /// contains unmatched delimiters.
    pub fn render_template(
        &self,
        template: &str,
        context: &HashMap<String, String>,
    ) -> Result<String, EngineError> {
        if template.trim().is_empty() {
            return Err(EngineError::InvalidTemplate(
                "Template is empty".to_string(),
            ));
        }

        // Check for single delimiters
        if template.contains(&self.open_delim[..1])
            && !template.contains(&self.open_delim)
        {
            return Err(EngineError::InvalidTemplate(format!(
                "Invalid template syntax: single '{}' are not allowed",
                &self.open_delim[..1]
            )));
        }

        let mut output = String::with_capacity(template.len());
        let mut last_end = 0;
        let mut depth = 0;

        for (idx, _) in template.match_indices(&self.open_delim) {
            if depth > 0 {
                return Err(EngineError::InvalidTemplate(
                    "Nested delimiters are not allowed".to_string(),
                ));
            }
            depth += 1;
            output.push_str(&template[last_end..idx]);
            if let Some(end) = template[idx..].find(&self.close_delim) {
                let key =
                    &template[idx + self.open_delim.len()..idx + end];
                if let Some(value) = context.get(key) {
                    output.push_str(value);
                } else {
                    return Err(EngineError::Render(format!(
                        "Unresolved template tag: {}",
                        key
                    )));
                }
                last_end = idx + end + self.close_delim.len();
                depth -= 1;
            } else {
                return Err(EngineError::InvalidTemplate(
                    "Unclosed template tag".to_string(),
                ));
            }
        }

        output.push_str(&template[last_end..]);

        Ok(output)
    }

    /// Sets custom delimiters for the template tags.
    ///
    /// This method allows you to define custom delimiters for the opening and closing
    /// tags in your templates. By default, the delimiters are `{{` for the opening tag
    /// and `}}` for the closing tag, but this method allows you to change them to any
    /// other strings. This can be useful when the default delimiters conflict with the
    /// content of your templates.
    ///
    /// # Arguments
    ///
    /// * `open` - The string to use as the opening delimiter (e.g., `<<`).
    /// * `close` - The string to use as the closing delimiter (e.g., `>>`).
    ///
    /// # Example
    ///
    /// ```
    /// use staticweaver::engine::Engine;
    /// use std::time::Duration;
    ///
    /// let mut engine = Engine::new("templates", Duration::from_secs(3600));
    ///
    /// // Set custom delimiters
    /// engine.set_delimiters("<<", ">>");
    ///
    /// // Now you can use the new delimiters in templates:
    /// let context = std::collections::HashMap::new();
    /// let template = "<<greeting>>, <<name>>!";
    /// let result = engine.render_template(template, &context);
    /// ```
    pub fn set_delimiters(&mut self, open: &str, close: &str) {
        self.open_delim = open.to_string();
        self.close_delim = close.to_string();
    }

    /// Creates or uses an existing template folder.
    ///
    /// This function checks if the template folder exists at the specified path. If a valid path is provided,
    /// it will check if the folder exists locally or download files from the given URL. If no path is provided,
    /// it uses a default URL to download the template folder.
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
    /// - `EngineError::Io`: If there is an issue with file operations (e.g., template directory not found, invalid UTF-8).
    /// - `EngineError::Reqwest`: If there is an issue downloading files from a URL (e.g., network request failure).
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
        let mut response = client
            .get(&file_url)
            .timeout(Duration::from_secs(10)) // Set a timeout
            .send()?;

        // Check if the response status is not a success (200-299)
        if !response.status().is_success() {
            // Return a custom error instead of trying to construct reqwest::Error
            return Err(EngineError::Render(format!(
                "Failed to download {}: HTTP {}",
                file,
                response.status()
            )));
        }

        // Proceed with file saving if the response is successful
        let mut file = File::create(&file_path)?;
        let _ = response.copy_to(&mut file)?;

        Ok(())
    }

    /// Clears all cached rendered templates.
    ///
    /// This method removes all entries from the cache, freeing up memory.
    /// After calling this, subsequent render requests will not retrieve
    /// any cached results and will regenerate the templates.
    ///
    /// # Example
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
    /// # Example
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_render_template() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("<<", ">>");
        let mut context = HashMap::new();
        let _ = context.insert("name".to_string(), "Alice".to_string());
        let _ =
            context.insert("greeting".to_string(), "Hello".to_string());

        let template = "<<greeting>>, <<name>>!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_render_template_empty() {
        let engine = Engine::new("", Duration::from_secs(60));
        let context = HashMap::new();

        let template = "";
        let result = engine.render_template(template, &context);
        assert!(matches!(result, Err(EngineError::InvalidTemplate(_))));
    }

    #[test]
    fn test_render_template_invalid_syntax() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("{{", "}}"); // Set back to default delimiters
        let context = HashMap::new();
        let template = "Hello, {name}!";
        let result = engine.render_template(template, &context);
        assert!(
            matches!(result, Err(EngineError::InvalidTemplate(msg)) if msg.contains("single '{'"))
        );
    }

    #[test]
    fn test_render_template_custom_delimiters() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("<<", ">>");
        let mut context = HashMap::new();
        let _ = context.insert("name".to_string(), "Alice".to_string());
        let _ =
            context.insert("greeting".to_string(), "Hello".to_string());

        let template = "<<greeting>>, <<name>>!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice!");

        // Test invalid syntax with custom delimiters
        let invalid_template = "Hello, <name>!";
        let result = engine.render_template(invalid_template, &context);
        assert!(
            matches!(result, Err(EngineError::InvalidTemplate(msg)) if msg.contains("single '<'"))
        );
    }

    #[test]
    fn test_render_template_unresolved_tag() {
        let engine = Engine::new("", Duration::from_secs(60));
        let context = HashMap::new();

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
    fn test_render_template_escaped_delimiters() {
        let mut engine = Engine::new("", Duration::from_secs(60));
        engine.set_delimiters("{{", "}}");
        let mut context = HashMap::new();
        let _ = context.insert("name".to_string(), "Alice".to_string());

        // Escaped delimiters should render as literal delimiters
        let template = "Hello, {{name}}!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice!");

        // Mixed usage: escaped and actual template tag
        let template = "Hello, {{name}}! Welcome, {{name}}!";
        let result =
            engine.render_template(template, &context).unwrap();
        assert_eq!(result, "Hello, Alice! Welcome, Alice!");
    }
}
