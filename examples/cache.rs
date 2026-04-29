// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! `Cache` operations: TTL, capacity, refresh, iteration.
//!
//! Run: `cargo run --example cache`

#[path = "support.rs"]
mod support;

use staticweaver::cache::Cache;
use std::thread::sleep;
use std::time::Duration;

fn main() {
    support::header("staticweaver -- cache");

    // ── Basic insert / get / remove ─────────────────────────────────
    let mut cache: Cache<String, i32> =
        Cache::new(Duration::from_secs(60));
    support::task("Insert `key1 = 42`", || {
        let _ = cache.insert("key1".to_string(), 42);
    });
    support::task_with_output("Read, check, remove", || {
        let value = cache.get(&"key1".to_string()).copied();
        let has_key2 = cache.contains_key(&"key2".to_string());
        let removed = cache.remove(&"key1".to_string());
        vec![
            format!("get     = {value:?}"),
            format!("has_key2 = {has_key2}"),
            format!("removed = {removed:?}"),
            format!("len     = {}", cache.len()),
        ]
    });

    // ── TTL expiration ──────────────────────────────────────────────
    let mut ttl_cache: Cache<String, String> =
        Cache::new(Duration::from_millis(100));
    support::task("Insert a short-lived entry", || {
        let _ = ttl_cache
            .insert("short".to_string(), "expires soon".to_string());
    });
    support::task_with_output("Observe TTL and expiry", || {
        sleep(Duration::from_millis(50));
        let remaining = ttl_cache.ttl(&"short".to_string());
        sleep(Duration::from_millis(60));
        let after = ttl_cache.get(&"short".to_string()).cloned();
        ttl_cache.remove_expired();
        vec![
            format!("ttl_at_50ms   = {remaining:?}"),
            format!("get_at_110ms = {after:?}"),
            format!("len_after_gc = {}", ttl_cache.len()),
        ]
    });

    // ── Bounded capacity ────────────────────────────────────────────
    let mut bounded: Cache<String, String> =
        Cache::with_capacity(Duration::from_secs(60), 2);
    support::task("Cap capacity at 2 entries (LRU eviction)", || {
        let _ = bounded.insert("k1".to_string(), "v1".to_string());
        let _ = bounded.insert("k2".to_string(), "v2".to_string());
        // Touching k1 promotes it
        let _ = bounded.get(&"k1".to_string());
        // Third insert evicts the LRU entry (k2)
        let _ = bounded.insert("k3".to_string(), "v3".to_string());
    });
    support::task_with_output(
        "Inspect stored keys (k2 should be gone)",
        || {
            let mut pairs: Vec<_> = bounded
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            pairs.sort();
            pairs
        },
    );

    // ── Refresh + update ────────────────────────────────────────────
    let mut refreshable: Cache<String, String> =
        Cache::new(Duration::from_millis(200));
    let _ = refreshable
        .insert("refresh".to_string(), "original".to_string());
    support::task_with_output("Refresh and update an entry", || {
        sleep(Duration::from_millis(150));
        let refreshed = refreshable.refresh(&"refresh".to_string());
        let updated = refreshable
            .update(&"refresh".to_string(), "updated".to_string());
        let current = refreshable.get(&"refresh".to_string()).cloned();
        vec![
            format!("refreshed = {refreshed}"),
            format!("updated  = {updated}"),
            format!("current  = {current:?}"),
        ]
    });

    // ── Iteration + IntoIterator ────────────────────────────────────
    let mut iter_cache: Cache<String, String> =
        Cache::new(Duration::from_secs(60));
    for i in 1..=3 {
        let _ = iter_cache.insert(format!("k{i}"), format!("v{i}"));
    }
    support::task_with_output(
        "Iterate, then consume via IntoIterator",
        || {
            let mut live: Vec<_> = iter_cache
                .iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            live.sort();
            let mut owned: Vec<_> = iter_cache
                .into_iter()
                .map(|(k, v)| format!("{k} = {v}"))
                .collect();
            owned.sort();
            let mut out = vec!["live (&)".to_string()];
            out.extend(live);
            out.push("owned (into)".to_string());
            out.extend(owned);
            out
        },
    );

    support::summary(7);
}
