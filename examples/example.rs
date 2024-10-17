// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Examples
//!
//! This module serves as an entry point for running all the StaticWeaver examples,
//! demonstrating various aspects of the library including context management,
//! template rendering, caching, and error handling.

mod cache_example;
mod context_example;
mod engine_example;
mod lib_example;

use std::error::Error;

/// Runs all StaticWeaver examples.
///
/// This function sequentially executes all individual examples, demonstrating
/// various features and capabilities of the StaticWeaver library.
fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🦀 Running StaticWeaver Examples 🦀");

    // Run the example modules
    cache_example::main()?;
    context_example::main()?;
    engine_example::main()?;
    lib_example::main()?;

    println!(
        "\n🎉 All StaticWeaver examples completed successfully!\n"
    );

    Ok(())
}
