// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # StaticWeaver Cache Examples
//!
//! This program demonstrates the usage of various features of the Cache struct
//! in the StaticWeaver library, including creation, insertion, retrieval, and expiration.

use staticweaver::cache::Cache;
use std::thread::sleep;
use std::time::Duration;

pub(crate) fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 StaticWeaver Cache Examples\n");

    basic_cache_operations()?;
    cache_expiration()?;
    cache_capacity()?;
    cache_refresh_and_update()?;
    cache_iteration()?;

    println!("\n🎉 All cache examples completed successfully!");

    Ok(())
}

/// Demonstrates basic cache operations like insertion, retrieval, and removal.
fn basic_cache_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("🦀 Basic Cache Operations");
    println!("---------------------------------------------");

    let mut cache: Cache<String, i32> =
        Cache::new(Duration::from_secs(60));

    let _ = cache.insert("key1".to_string(), 42);
    println!("    ✅ Inserted 'key1' with value 42");

    match cache.get(&"key1".to_string()) {
        Some(&value) => println!("    ✅ Retrieved 'key1': {}", value),
        None => println!("    ❌ Failed to retrieve 'key1'"),
    }

    println!(
        "    ✅ 'key1' exists: {}",
        cache.contains_key(&"key1".to_string())
    );
    println!(
        "    ✅ 'key2' exists: {}",
        cache.contains_key(&"key2".to_string())
    );

    match cache.remove(&"key1".to_string()) {
        Some(value) => {
            println!("    ✅ Removed 'key1' with value: {}", value)
        }
        None => println!("    ❌ Failed to remove 'key1'"),
    }

    println!("    ✅ Cache size: {}", cache.len());
    println!("    ✅ Is cache empty? {}", cache.is_empty());

    Ok(())
}

/// Demonstrates cache expiration behavior.
fn cache_expiration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Cache Expiration");
    println!("---------------------------------------------");

    let mut cache = Cache::new(Duration::from_millis(100));

    let _ = cache.insert(
        "short_lived".to_string(),
        "I'll expire soon".to_string(),
    );
    println!("    ✅ Inserted 'short_lived' key");

    sleep(Duration::from_millis(50));

    match cache.ttl(&"short_lived".to_string()) {
        Some(ttl) => {
            println!("    ✅ Time-to-live for 'short_lived': {:?}", ttl)
        }
        None => println!("    ❌ Failed to get TTL for 'short_lived'"),
    }

    sleep(Duration::from_millis(60));

    match cache.get(&"short_lived".to_string()) {
        Some(_) => println!(
            "    ❌ Unexpected: 'short_lived' key still exists"
        ),
        None => println!("    ✅ 'short_lived' key has expired"),
    }

    cache.remove_expired();
    println!(
        "    ✅ Removed expired entries. Cache size: {}",
        cache.len()
    );

    Ok(())
}

/// Demonstrates cache behavior with capacity limits.
fn cache_capacity() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Cache Capacity");
    println!("---------------------------------------------");

    let mut cache: Cache<String, String> =
        Cache::with_capacity(Duration::from_secs(60), 2);

    let _ = cache.insert("key1".to_string(), "value1".to_string());
    let _ = cache.insert("key2".to_string(), "value2".to_string());
    println!("    ✅ Inserted 'key1' and 'key2'");

    let _ = cache.insert("key3".to_string(), "value3".to_string());
    println!("    ✅ Attempted to insert 'key3'");

    println!("    ✅ Cache contents:");
    for (key, value) in cache.iter() {
        println!("       {}: {}", key, value);
    }

    Ok(())
}

/// Demonstrates refreshing and updating cache entries.
fn cache_refresh_and_update() -> Result<(), Box<dyn std::error::Error>>
{
    println!("\n🦀 Cache Refresh and Update");
    println!("---------------------------------------------");

    let mut cache = Cache::new(Duration::from_millis(200));

    let _ = cache
        .insert("refresh_me".to_string(), "original value".to_string());
    println!("    ✅ Inserted 'refresh_me'");

    sleep(Duration::from_millis(150));

    match cache.refresh(&"refresh_me".to_string()) {
        true => println!("    ✅ Refreshed 'refresh_me'"),
        false => println!("    ❌ Failed to refresh 'refresh_me'"),
    }

    match cache
        .update(&"refresh_me".to_string(), "updated value".to_string())
    {
        true => println!("    ✅ Updated 'refresh_me'"),
        false => println!("    ❌ Failed to update 'refresh_me'"),
    }

    match cache.get(&"refresh_me".to_string()) {
        Some(value) => {
            println!("    ✅ Current value of 'refresh_me': {}", value)
        }
        None => println!("    ❌ Failed to get 'refresh_me'"),
    }

    Ok(())
}

/// Demonstrates iterating over cache entries.
fn cache_iteration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🦀 Cache Iteration");
    println!("---------------------------------------------");

    let mut cache = Cache::new(Duration::from_secs(60));

    for i in 1..=5 {
        let _ =
            cache.insert(format!("key{}", i), format!("value{}", i));
    }

    println!("    ✅ Iterating over cache entries:");
    for (key, value) in cache.iter() {
        println!("       {}: {}", key, value);
    }

    println!("\n    ✅ Converting cache to a vector:");
    let vec: Vec<(String, String)> = cache.into_iter().collect();
    for (key, value) in &vec {
        println!("       {}: {}", key, value);
    }

    Ok(())
}
