// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Error types for the StaticWeaver library.
//!
//! This module defines custom error types used throughout the library,
//! providing detailed error information and context for various failure scenarios.

use std::io;
use thiserror::Error;

/// Represents high-level errors that can occur during the operation of the engine.
///
/// This error type consolidates multiple underlying error types, including I/O errors,
/// network request errors, and rendering-specific issues. It provides a unified interface for
/// handling errors in the engine context.
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
}

/// Represents errors specific to template processing.
///
/// This error type focuses on issues related to the manipulation of templates,
/// such as syntax errors, rendering failures, or invalid input data.
#[derive(Error, Debug)]
pub enum TemplateError {
    /// I/O error encountered during template operations.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Network request error encountered during template operations.
    #[error("Request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Error triggered by invalid template syntax.
    #[error("Invalid template syntax: {0}")]
    InvalidSyntax(String),

    /// Error during rendering, such as unresolved template tags or missing context.
    #[error("Rendering error: {0}")]
    RenderError(String),

    /// Encountered an engine error during the template processing.
    #[error("Engine error: {0}")]
    EngineError(#[from] Box<EngineError>),
}

/// A specialized `Result` type for StaticWeaver operations.
///
/// This type is used throughout the StaticWeaver library for any operation that
/// can produce an `EngineError`.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_error_display() {
        let err = EngineError::Render(
            "Failed to render template".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Render error: Failed to render template"
        );
    }

    #[test]
    fn test_template_error_display() {
        let err =
            TemplateError::InvalidSyntax("Unclosed tag".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid template syntax: Unclosed tag"
        );
    }

    #[test]
    fn test_error_conversion() {
        let io_err =
            io::Error::new(io::ErrorKind::NotFound, "File not found");
        let engine_err: EngineError = io_err.into();
        assert!(matches!(engine_err, EngineError::Io(_)));
    }
}
