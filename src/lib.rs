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

/// The `context` module contains the `Context` struct, which is used to store and manage template variables.
pub mod context;

/// The `engine` module contains the `Engine` struct, which is used to render templates.
pub mod engine;

/// The `error` module contains the `TemplateError` enum, which represents errors that can occur during template processing.
pub mod error;

/// The `cache` module contains the `Cache` struct, which is used to cache rendered templates for improved performance.
pub mod cache;

pub use context::Context;
pub use engine::{Engine, PageOptions};
pub use error::EngineError;
pub use error::TemplateError;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{Context, Engine, TemplateError};
}
