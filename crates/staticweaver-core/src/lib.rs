// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://cloudcdn.pro/staticweaver/v1/favicon.ico",
    html_logo_url = "https://cloudcdn.pro/staticweaver/v1/logos/staticweaver.svg",
    html_root_url = "https://docs.rs/staticweaver-core"
)]
#![deny(missing_docs)]

extern crate alloc;

/// Polymorphic key-value [`Context`] and the [`Value`] enum (`Null`,
/// `Bool`, `Number`, `String`, `List`, `Map`) that templates substitute.
pub mod context;

/// Error types ([`EngineError`] and [`error::TemplateError`]) returned
/// by every fallible operation.
pub mod error;

/// Template-name containment rules.
pub mod path;

mod escape;
mod expr;
mod expr_parse;
mod filters;
mod loader;
mod num;
mod render;
mod syntax;

#[cfg(test)]
mod unit_tests;

pub use context::{Context, Value};
pub use error::{EngineError, TemplateError};
pub use loader::{FilterFn, Registry, TemplateLoader, TestFn};
pub use render::Renderer;
