use crate::engine;
use std::io;
use thiserror::Error;

/// `EngineError` represents high-level errors that can occur during the operation of the engine.
///
/// This error type consolidates multiple underlying error types, including I/O errors,
/// network request errors, and rendering-specific issues. It provides a unified interface for
/// handling errors in the engine context.
///
/// # Variants
/// - `Io`: Represents errors related to I/O operations, such as file reading or writing.
/// - `Reqwest`: Represents errors from the `reqwest` crate related to HTTP requests.
/// - `Render`: Occurs when rendering a template fails due to unresolved tags or other issues.
/// - `InvalidTemplate`: Triggered when the template contains syntax issues, such as unclosed tags.
/// - `Template`: Captures errors specific to template processing via `TemplateError`.
/// - `Engine`: Encapsulates lower-level `EngineError` for composability.
#[derive(Error, Debug)]
pub enum EngineError {
    /// I/O error encountered during engine operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Network request error encountered during engine operations.
    #[error("Request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Error occurring during the rendering process.
    #[error("Render error: {0}")]
    Render(String),

    /// Error triggered by invalid template syntax.
    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    /// Template-specific error, such as invalid syntax or rendering issues.
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    /// Encapsulates another `EngineError`, useful for higher-level errors in engine processing.
    #[error("Engine error: {0}")]
    Engine(#[from] engine::EngineError),
}

/// `TemplateError` represents errors specific to template processing.
///
/// This error type focuses on issues related to the manipulation of templates,
/// such as syntax errors, rendering failures, or invalid input data.
/// It also consolidates I/O and HTTP request errors for template-related operations.
///
/// # Variants
/// - `Io`: Represents I/O-related errors, such as reading or writing template files.
/// - `Reqwest`: Represents errors from the `reqwest` crate related to fetching templates over HTTP.
/// - `InvalidSyntax`: Raised when the template has invalid syntax, such as unclosed delimiters.
/// - `RenderError`: Raised when a rendering issue occurs due to missing or incorrect template data.
/// - `Engine`: Encapsulates lower-level `EngineError` when they affect template operations.
#[derive(Error, Debug)]
pub enum TemplateError {
    /// I/O error encountered during template operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Network request error encountered during template operations.
    #[error("Request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Error triggered by invalid template syntax.
    #[error("Invalid template syntax")]
    InvalidSyntax,

    /// Error during rendering, such as unresolved template tags or missing context.
    #[error("Rendering error: {0}")]
    RenderError(String),

    /// Engine-level error that affects the template operations.
    #[error("Engine error: {0}")]
    Engine(#[from] engine::EngineError),
}
