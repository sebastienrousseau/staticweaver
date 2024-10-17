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
#[non_exhaustive]
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

    /// Error when a required resource is not found.
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// Error when an operation times out.
    #[error("Operation timed out: {0}")]
    Timeout(String),
}

/// Represents errors specific to template processing.
///
/// This error type focuses on issues related to the manipulation of templates,
/// such as syntax errors, rendering failures, or invalid input data.
#[derive(Error, Debug)]
#[non_exhaustive]
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

    /// Error when a required variable is missing from the context.
    #[error("Missing variable: {0}")]
    MissingVariable(String),

    /// Error when an invalid operation is attempted on a template.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
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

    #[test]
    fn test_resource_not_found_error() {
        let err =
            EngineError::ResourceNotFound("template.html".to_string());
        assert_eq!(
            err.to_string(),
            "Resource not found: template.html"
        );
    }

    #[test]
    fn test_timeout_error() {
        let err = EngineError::Timeout(
            "Fetching remote template".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Operation timed out: Fetching remote template"
        );
    }

    #[test]
    fn test_missing_variable_error() {
        let err =
            TemplateError::MissingVariable("user_name".to_string());
        assert_eq!(err.to_string(), "Missing variable: user_name");
    }

    #[test]
    fn test_invalid_operation_error() {
        let err = TemplateError::InvalidOperation(
            "Nested templates not allowed".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Invalid operation: Nested templates not allowed"
        );
    }

    #[test]
    fn test_engine_error_io() {
        let io_err =
            io::Error::new(io::ErrorKind::NotFound, "File not found");
        let err = EngineError::Io(io_err);
        assert_eq!(err.to_string(), "I/O error: File not found");
    }

    #[test]
    fn test_engine_error_render() {
        let err = EngineError::Render(
            "Failed to render template".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Render error: Failed to render template"
        );
    }

    #[test]
    fn test_engine_error_invalid_template() {
        let err =
            EngineError::InvalidTemplate("Invalid syntax".to_string());
        assert_eq!(err.to_string(), "Invalid template: Invalid syntax");
    }

    #[test]
    fn test_engine_error_template() {
        let template_err =
            TemplateError::InvalidSyntax("Unclosed tag".to_string());
        let err = EngineError::Template(template_err);
        assert_eq!(
            err.to_string(),
            "Template error: Invalid template syntax: Unclosed tag"
        );
    }

    #[test]
    fn test_engine_error_resource_not_found() {
        let err =
            EngineError::ResourceNotFound("template.html".to_string());
        assert_eq!(
            err.to_string(),
            "Resource not found: template.html"
        );
    }

    #[test]
    fn test_engine_error_timeout() {
        let err = EngineError::Timeout(
            "Fetching remote template".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Operation timed out: Fetching remote template"
        );
    }

    // TemplateError tests

    #[test]
    fn test_template_error_io() {
        let io_err = io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Permission denied",
        );
        let err = TemplateError::Io(io_err);
        assert_eq!(err.to_string(), "I/O error: Permission denied");
    }

    #[test]
    fn test_template_error_invalid_syntax() {
        let err =
            TemplateError::InvalidSyntax("Unclosed tag".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid template syntax: Unclosed tag"
        );
    }

    #[test]
    fn test_template_error_render_error() {
        let err = TemplateError::RenderError(
            "Missing context variable".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Rendering error: Missing context variable"
        );
    }

    #[test]
    fn test_template_error_engine_error() {
        let engine_err =
            EngineError::Render("Render failure".to_string());
        let err = TemplateError::EngineError(Box::new(engine_err));
        assert_eq!(
            err.to_string(),
            "Engine error: Render error: Render failure"
        );
    }

    #[test]
    fn test_template_error_missing_variable() {
        let err =
            TemplateError::MissingVariable("user_name".to_string());
        assert_eq!(err.to_string(), "Missing variable: user_name");
    }

    #[test]
    fn test_template_error_invalid_operation() {
        let err = TemplateError::InvalidOperation(
            "Nested templates not allowed".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "Invalid operation: Nested templates not allowed"
        );
    }

    #[test]
    fn test_engine_error_conversion_from_io_error() {
        let io_err =
            io::Error::new(io::ErrorKind::NotFound, "File not found");
        let engine_err: EngineError = io_err.into();
        assert!(matches!(engine_err, EngineError::Io(_)));
    }

    #[test]
    fn test_template_error_conversion_from_io_error() {
        let io_err = io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Permission denied",
        );
        let template_err: TemplateError = io_err.into();
        assert!(matches!(template_err, TemplateError::Io(_)));
    }
}
