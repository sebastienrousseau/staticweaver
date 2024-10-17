// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Library Usage Example
//!
//! This example demonstrates the usage of the main components
//! of the StaticWeaver library, including Context, Engine,
//! PageOptions, and error handling.

use staticweaver::{engine::EngineError, prelude::*, PageOptions};
use std::time::Duration;

pub(crate) fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StaticWeaver Library Usage Example\n");

    context_usage()?;
    engine_usage()?;
    page_options_usage()?;
    error_handling()?;

    println!(
        "\n🎉 All StaticWeaver library examples completed successfully!"
    );

    Ok(())
}

/// Demonstrates the usage of the Context struct.
fn context_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("🦀 Context Usage");
    println!("---------------------------------------------");

    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    context.set("age".to_string(), "30".to_string());

    println!("    ✅ Created context with name and age");

    match context.get("name") {
        Some(name) => {
            println!("    ✅ Retrieved name from context: {}", name)
        }
        None => println!("    ❌ Failed to retrieve name from context"),
    }

    println!("    ✅ Context size: {}", context.len());

    Ok(())
}

/// Demonstrates the usage of the Engine struct.
fn engine_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Engine Usage");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("greeting".to_string(), "Hello".to_string());
    context.set("name".to_string(), "World".to_string());

    let template = "{{greeting}}, {{name}}!";

    match engine.render_template(template, &context) {
        Ok(result) => println!("    ✅ Rendered template: {}", result),
        Err(e) => println!("    ❌ Failed to render template: {:?}", e),
    }

    Ok(())
}

/// Demonstrates the usage of PageOptions.
fn page_options_usage() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 PageOptions Usage");
    println!("---------------------------------------------");

    let mut options = PageOptions::new();
    options
        .set("title".to_string(), "My StaticWeaver Page".to_string());
    options.set(
        "description".to_string(),
        "A sample page created with StaticWeaver".to_string(),
    );

    println!("    ✅ Created PageOptions with title and description");

    match options.get("title") {
        Some(title) => println!(
            "    ✅ Retrieved title from PageOptions: {}",
            title
        ),
        None => {
            println!("    ❌ Failed to retrieve title from PageOptions")
        }
    }

    Ok(())
}

/// Demonstrates error handling with EngineError and TemplateError.
fn error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Error Handling");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));
    let context = Context::new();

    // Deliberately use an invalid template to trigger an error
    let invalid_template = "{{greeting}, {{name}!";

    match engine.render_template(invalid_template, &context) {
        Ok(_) => {
            println!("    ❌ Unexpected success with invalid template")
        }
        Err(EngineError::InvalidTemplate(msg)) => {
            println!("    ✅ Caught InvalidTemplate error: {}", msg)
        }
        Err(e) => println!("    ❌ Unexpected error type: {:?}", e),
    }

    // Demonstrate TemplateError usage
    let template_error = TemplateError::InvalidSyntax(
        "Missing closing delimiter".to_string(),
    );
    println!("    ✅ Created TemplateError: {:?}", template_error);

    Ok(())
}
