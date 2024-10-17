// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

// src/lib.rs

#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://kura.pro/staticweaver/images/favicon.ico",
    html_logo_url = "https://kura.pro/staticweaver/images/logos/staticweaver.svg",
    html_root_url = "https://docs.rs/staticweaver"
)]
#![crate_name = "staticweaver"]
#![crate_type = "lib"]
#![deny(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

/// Contains the `Context` struct for managing template variables.
pub mod context;

/// Provides the `Engine` struct for template rendering.
pub mod engine;

/// Defines error types for template processing.
pub mod error;

/// Implements caching mechanisms for improved performance.
pub mod cache;

pub use context::Context;
pub use engine::{Engine, PageOptions};
pub use error::{EngineError, TemplateError};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{Context, Engine, EngineError, TemplateError};
}
