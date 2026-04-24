// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Error Handling Examples
//!
//! This program demonstrates the usage of various error types and functions
//! in the StaticWeaver library's engine module, including creating and handling
//! different types of EngineError.

use staticweaver::engine::{Engine, EngineError};
use staticweaver::Context;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

pub(crate) fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StaticWeaver Error Handling Examples\n");

    io_error_example()?;
    reqwest_error_example()?;
    render_error_example()?;
    invalid_template_example()?;
    template_rendering_example()?;

    println!(
        "\n🎉 All error handling examples completed successfully!"
    );

    Ok(())
}

/// Demonstrates handling I/O errors.
fn io_error_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("🦀 I/O Error Example");
    println!("---------------------------------------------");

    let mut engine =
        Engine::new("nonexistent_path", Duration::from_secs(60));
    let context = Context::new();

    match engine.render_page(&context, "nonexistent_template") {
        Ok(_) => println!("    ✅ Unexpected success"),
        Err(e) => match e {
            EngineError::Io(io_error) => {
                println!("    ❌ I/O Error: {}", io_error)
            }
            _ => println!("    ❌ Unexpected error type: {:?}", e),
        },
    }

    Ok(())
}

/// Demonstrates handling network request errors. Only compiled when the
/// `remote-templates` feature is enabled.
#[cfg(feature = "remote-templates")]
fn reqwest_error_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Reqwest Error Example");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));

    match engine.create_template_folder(Some("https://nonexistent.url"))
    {
        Ok(_) => println!("    ✅ Unexpected success"),
        Err(e) => match e {
            EngineError::Reqwest(req_error) => {
                println!("    ❌ Reqwest Error: {}", req_error)
            }
            _ => println!("    ❌ Unexpected error type: {:?}", e),
        },
    }

    Ok(())
}

/// Stub used when the `remote-templates` feature is disabled — keeps the
/// example binary building with a flat feature matrix.
#[cfg(not(feature = "remote-templates"))]
fn reqwest_error_example() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "\n🦀 Reqwest example skipped (build with --features remote-templates)"
    );
    Ok(())
}

/// Demonstrates handling render errors.
fn render_error_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Render Error Example");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));
    let context = Context::new();
    let template = "Hello, {{name}}!";

    match engine.render_template(template, &context) {
        Ok(_) => println!("    ✅ Unexpected success"),
        Err(e) => match e {
            EngineError::Render(msg) => {
                println!("    ❌ Render Error: {}", msg)
            }
            _ => println!("    ❌ Unexpected error type: {:?}", e),
        },
    }

    Ok(())
}

/// Demonstrates handling invalid template syntax errors.
fn invalid_template_example() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Invalid Template Example");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));
    let context = Context::new();
    let template = "Hello, {name}!"; // Invalid syntax

    match engine.render_template(template, &context) {
        Ok(_) => println!("    ✅ Unexpected success"),
        Err(e) => match e {
            EngineError::InvalidTemplate(msg) => {
                println!("    ❌ Invalid Template Error: {}", msg)
            }
            _ => println!("    ❌ Unexpected error type: {:?}", e),
        },
    }

    Ok(())
}

/// Demonstrates a complete template rendering scenario with potential errors.
fn template_rendering_example() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Template Rendering Example");
    println!("---------------------------------------------");

    let temp_dir = TempDir::new()?;
    let template_path = temp_dir.path().join("template.html");
    fs::write(&template_path, "Hello, {{name}}!")?;

    let mut engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut context = Context::new();
    context.set("name".to_string(), "World".to_string());

    match engine.render_page(&context, "template") {
        Ok(result) => println!("    ✅ Rendered template: {}", result),
        Err(e) => match e {
            EngineError::Io(io_error) => {
                println!("    ❌ I/O Error: {}", io_error)
            }
            EngineError::Render(msg) => {
                println!("    ❌ Render Error: {}", msg)
            }
            EngineError::InvalidTemplate(msg) => {
                println!("    ❌ Invalid Template Error: {}", msg)
            }
            #[cfg(feature = "remote-templates")]
            EngineError::Reqwest(err) => {
                println!("    ❌ Reqwest Error: {}", err)
            }
        },
    }

    Ok(())
}
