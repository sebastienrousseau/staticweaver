// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Engine Examples
//!
//! This program demonstrates the usage of the Engine struct
//! in the StaticWeaver library, including template rendering,
//! page rendering, and various engine operations.

use staticweaver::engine::{Engine, EngineError};
use staticweaver::Context;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

pub(crate) fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StaticWeaver Engine Examples\n");

    basic_template_rendering()?;
    page_rendering()?;
    custom_delimiters()?;
    cache_operations()?;
    template_folder_creation()?;

    println!("\n🎉 All engine examples completed successfully!");

    Ok(())
}

/// Demonstrates basic template rendering.
fn basic_template_rendering() -> Result<(), Box<dyn std::error::Error>>
{
    println!("🦀 Basic Template Rendering");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    context.set("greeting".to_string(), "Hello".to_string());

    let template = "{{greeting}}, {{name}}!";

    match engine.render_template(template, &context) {
        Ok(result) => println!("    ✅ Rendered template: {}", result),
        Err(e) => println!("    ❌ Failed to render template: {:?}", e),
    }

    // Test error case with unresolved tag
    let template_with_error = "{{greeting}}, {{unresolved}}!";
    match engine.render_template(template_with_error, &context) {
        Ok(_) => {
            println!("    ❌ Unexpected success with unresolved tag")
        }
        Err(EngineError::Render(msg)) => {
            println!("    ✅ Expected error: {}", msg)
        }
        Err(e) => println!("    ❌ Unexpected error type: {:?}", e),
    }

    Ok(())
}

/// Demonstrates page rendering with file-based templates.
fn page_rendering() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Page Rendering");
    println!("---------------------------------------------");

    let temp_dir = TempDir::new()?;
    let template_path = temp_dir.path().join("layout.html");
    fs::write(&template_path, "<html><body>{{content}}</body></html>")?;

    let mut engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut context = Context::new();
    context.set(
        "content".to_string(),
        "Welcome to StaticWeaver!".to_string(),
    );

    match engine.render_page(&context, "layout") {
        Ok(result) => println!("    ✅ Rendered page: {}", result),
        Err(e) => println!("    ❌ Failed to render page: {:?}", e),
    }

    Ok(())
}

/// Demonstrates using custom delimiters for templates.
fn custom_delimiters() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Custom Delimiters");
    println!("---------------------------------------------");

    let mut engine = Engine::new("templates", Duration::from_secs(60));
    engine.set_delimiters("<<", ">>");

    let mut context = Context::new();
    context.set("name".to_string(), "Bob".to_string());

    let template = "Hello, <<name>>!";

    match engine.render_template(template, &context) {
        Ok(result) => println!(
            "    ✅ Rendered with custom delimiters: {}",
            result
        ),
        Err(e) => println!(
            "    ❌ Failed to render with custom delimiters: {:?}",
            e
        ),
    }

    Ok(())
}

/// Demonstrates cache operations.
fn cache_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Cache Operations");
    println!("---------------------------------------------");

    let mut engine = Engine::new("templates", Duration::from_secs(60));
    let context = Context::new();

    // Simulate caching by rendering the same template twice
    let template = "Cached content";
    let _ = engine.render_template(template, &context)?;
    let _ = engine.render_template(template, &context)?;

    println!("    ✅ Cache size: {}", engine.render_cache.len());

    engine.clear_cache();
    println!(
        "    ✅ Cache cleared. New size: {}",
        engine.render_cache.len()
    );

    engine.set_max_cache_size(10);
    println!("    ✅ Set maximum cache size to 10");

    Ok(())
}

/// Demonstrates template folder creation.
fn template_folder_creation() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Template Folder Creation");
    println!("---------------------------------------------");

    let engine = Engine::new("templates", Duration::from_secs(60));

    // Test with a local path
    match engine.create_template_folder(Some("test_templates")) {
        Ok(path) => {
            println!("    ✅ Created local template folder: {}", path)
        }
        Err(e) => println!(
            "    ❌ Failed to create local template folder: {:?}",
            e
        ),
    }

    // Test with a URL (this will attempt to download templates)
    let url = "https://raw.githubusercontent.com/sebastienrousseau/shokunin/main/template/";
    match engine.create_template_folder(Some(url)) {
        Ok(path) => {
            println!("    ✅ Downloaded templates to: {}", path)
        }
        Err(e) => {
            println!("    ❌ Failed to download templates: {:?}", e)
        }
    }

    Ok(())
}
