// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Context Examples
//!
//! This program demonstrates the usage of the Context struct
//! in the StaticWeaver library, including creation, manipulation,
//! and various operations on context data.

use staticweaver::Context;

pub(crate) fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StaticWeaver Context Examples\n");

    basic_context_operations()?;
    context_capacity_and_clear()?;
    context_update_and_remove()?;
    context_iteration()?;
    context_from_iterator()?;
    context_hashing()?;

    println!("\n🎉 All context examples completed successfully!");

    Ok(())
}

/// Demonstrates basic Context operations like creation, setting, and getting values.
fn basic_context_operations() -> Result<(), Box<dyn std::error::Error>>
{
    println!("🦀 Basic Context Operations");
    println!("---------------------------------------------");

    let mut context = Context::new();

    context.set("name".to_string(), "Alice".to_string());
    context.set("age".to_string(), "30".to_string());

    match context.get("name") {
        Some(value) => println!("    ✅ Retrieved name: {}", value),
        None => println!("    ❌ Failed to retrieve name"),
    }

    match context.get("age") {
        Some(value) => println!("    ✅ Retrieved age: {}", value),
        None => println!("    ❌ Failed to retrieve age"),
    }

    match context.get("occupation") {
        Some(_) => {
            println!("    ❌ Unexpected: retrieved non-existent key")
        }
        None => println!(
            "    ✅ Correctly failed to retrieve non-existent key"
        ),
    }

    println!("    ✅ Context size: {}", context.len());

    Ok(())
}

/// Demonstrates context capacity operations and clearing the context.
fn context_capacity_and_clear() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Context Capacity and Clear");
    println!("---------------------------------------------");

    let mut context = Context::with_capacity(10);
    println!("    ✅ Created context with capacity >= 10");

    context.set("key1".to_string(), "value1".to_string());
    context.set("key2".to_string(), "value2".to_string());

    println!("    ✅ Context size before clear: {}", context.len());
    println!("    ✅ Is context empty? {}", context.is_empty());

    context.clear();
    println!("    ✅ Cleared context");
    println!("    ✅ Context size after clear: {}", context.len());
    println!("    ✅ Is context empty? {}", context.is_empty());

    Ok(())
}

/// Demonstrates updating and removing entries in the context.
fn context_update_and_remove() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Context Update and Remove");
    println!("---------------------------------------------");

    let mut context = Context::new();

    context.set("color".to_string(), "blue".to_string());
    println!("    ✅ Set color to blue");

    context.update("color", "red");
    match context.get("color") {
        Some(value) => println!("    ✅ Updated color: {}", value),
        None => println!("    ❌ Failed to update color"),
    }

    match context.remove("color") {
        Some(value) => println!("    ✅ Removed color: {}", value),
        None => println!("    ❌ Failed to remove color"),
    }

    match context.get("color") {
        Some(_) => println!(
            "    ❌ Unexpected: color still exists after removal"
        ),
        None => println!("    ✅ Color successfully removed"),
    }

    Ok(())
}

/// Demonstrates iterating over context entries.
fn context_iteration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Context Iteration");
    println!("---------------------------------------------");

    let mut context = Context::new();
    context.set("name".to_string(), "Bob".to_string());
    context.set("age".to_string(), "25".to_string());
    context.set("city".to_string(), "New York".to_string());

    println!("    ✅ Iterating over context entries:");
    for (key, value) in context.iter() {
        println!("       {}: {}", key, value);
    }

    Ok(())
}

/// Demonstrates creating a context from an iterator.
fn context_from_iterator() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Context from Iterator");
    println!("---------------------------------------------");

    let pairs = vec![
        ("fruit".to_string(), "apple".to_string()),
        ("vegetable".to_string(), "carrot".to_string()),
    ];

    let context: Context = pairs.into_iter().collect();

    match context.get("fruit") {
        Some(value) => println!("    ✅ Retrieved fruit: {}", value),
        None => println!("    ❌ Failed to retrieve fruit"),
    }

    match context.get("vegetable") {
        Some(value) => {
            println!("    ✅ Retrieved vegetable: {}", value)
        }
        None => println!("    ❌ Failed to retrieve vegetable"),
    }

    Ok(())
}

/// Demonstrates context hashing functionality.
fn context_hashing() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Context Hashing");
    println!("---------------------------------------------");

    let mut context1 = Context::new();
    context1.set("key".to_string(), "value".to_string());

    let mut context2 = Context::new();
    context2.set("key".to_string(), "value".to_string());

    let hash1 = context1.hash();
    let hash2 = context2.hash();

    println!("    ✅ Hash of context1: {}", hash1);
    println!("    ✅ Hash of context2: {}", hash2);

    if hash1 == hash2 {
        println!("    ✅ Hashes are equal for identical contexts");
    } else {
        println!("    ❌ Unexpected: Hashes are not equal for identical contexts");
    }

    context2
        .set("another_key".to_string(), "another_value".to_string());
    let hash3 = context2.hash();

    if hash1 != hash3 {
        println!("    ✅ Hashes are different after modifying context");
    } else {
        println!("    ❌ Unexpected: Hashes are equal after modifying context");
    }

    Ok(())
}
