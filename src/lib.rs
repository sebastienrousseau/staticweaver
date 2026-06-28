#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://cloudcdn.pro/staticweaver/v1/favicon.ico",
    html_logo_url = "https://cloudcdn.pro/staticweaver/v1/logos/staticweaver.svg",
    html_root_url = "https://docs.rs/staticweaver"
)]
#![crate_name = "staticweaver"]
#![crate_type = "lib"]
#![deny(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

/// Polymorphic key-value `Context` and the `Value` enum (`Null`,
/// `Bool`, `Number`, `String`, `List`, `Map`) that templates substitute.
/// Supports dot-notation lookup via [`context::Context::get_path`].
pub mod context;

/// The [`engine::Engine`] struct: template parser, expression
/// evaluator, partial loader, inheritance resolver, filter pipeline,
/// and renderer for both in-memory strings and `.html` files on disk.
pub mod engine;

/// Error types ([`error::EngineError`] and [`error::TemplateError`])
/// returned by every fallible operation in the crate.
pub mod error;

/// Generic time-bounded LRU cache with TTL expiration. Used by
/// [`engine::Engine::render_page`] to memoise rendered pages.
pub mod cache;

/// Async template-loading surface (issue #37). Gated behind the
/// `async` feature so default builds stay sync-only. Exposes the
/// [`loader_async::AsyncTemplateLoader`] trait and a reference
/// [`loader_async::TokioFsLoader`] impl (under `async-tokio`).
#[cfg(feature = "async")]
#[cfg_attr(docsrs, doc(cfg(feature = "async")))]
pub mod loader_async;

pub use context::Context;
pub use engine::Engine;
pub use error::{EngineError, TemplateError};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{Context, Engine, EngineError, TemplateError};
}
