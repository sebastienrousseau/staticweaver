// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! `Context` operations: insert, update, remove, iterate, hash.
//!
//! Run: `cargo run --example context`

#[path = "support.rs"]
mod support;

use staticweaver::Context;

fn main() {
    support::header("staticweaver -- context");

    // ── Basic set / get ─────────────────────────────────────────────
    let mut context = Context::new();
    support::task("Insert two keys", || {
        context.set("name".to_string(), "Alice".to_string());
        context.set("age".to_string(), "30".to_string());
    });

    support::task_with_output("Read the inserted values", || {
        vec![
            format!("name = {:?}", context.get("name")),
            format!("age  = {:?}", context.get("age")),
            format!("other = {:?}", context.get("occupation")),
            format!("len  = {}", context.len()),
        ]
    });

    // ── Capacity + clear ────────────────────────────────────────────
    let mut sized = Context::with_capacity(10);
    support::task("Create a context with capacity 10", || {
        sized.set("k1".to_string(), "v1".to_string());
        sized.set("k2".to_string(), "v2".to_string());
    });
    support::task_with_output("Clear it", || {
        let before = sized.len();
        sized.clear();
        vec![format!("before = {before}, after = {}", sized.len())]
    });

    // ── Update / remove ─────────────────────────────────────────────
    let mut colour = Context::new();
    colour.set("colour".to_string(), "blue".to_string());
    support::task("Update `colour` to `red`", || {
        colour.update("colour", "red");
    });
    support::task_with_output("Remove `colour`", || {
        let removed = colour.remove("colour");
        vec![
            format!("removed = {removed:?}"),
            format!("present_after = {:?}", colour.get("colour")),
        ]
    });

    // ── Typed Values ────────────────────────────────────────────────
    let mut typed = Context::new();
    support::task("Insert typed values", || {
        typed.set_value("active".to_string(), true);
        typed.set_value("count".to_string(), 42);
        typed.set_value("tags".to_string(), vec!["rust", "ssg"]);
    });

    support::task_with_output("Read typed values", || {
        vec![
            format!("active = {:?}", typed.get_value("active")),
            format!("count  = {:?}", typed.get_value("count")),
            format!(
                "truthy? = {}",
                typed
                    .get_value("active")
                    .is_some_and(|v| v.is_truthy())
            ),
        ]
    });

    // ── Nested Data & Paths ─────────────────────────────────────────
    let mut nested = Context::new();
    support::task("Insert nested Map", || {
        let mut user = fnv::FnvHashMap::default();
        let _ = user.insert("name".to_string(), "Ada".into());
        let _ = user.insert("role".to_string(), "Admin".into());
        nested.set_value(
            "user".to_string(),
            staticweaver::context::Value::Map(user),
        );
    });

    support::task_with_output("Resolve paths: `user.name`", || {
        vec![
            format!("name = {:?}", nested.get_path("user.name")),
            format!("role = {:?}", nested.get_path("user.role")),
            format!("miss = {:?}", nested.get_path("user.age")),
        ]
    });

    // ── Iteration ───────────────────────────────────────────────────
    let mut people = Context::new();
    people.set("name".to_string(), "Bob".to_string());
    people.set("age".to_string(), "25".to_string());
    people.set("city".to_string(), "New York".to_string());
    support::task_with_output("Iterate over entries", || {
        let mut pairs: Vec<_> =
            people.iter().map(|(k, v)| format!("{k} = {v}")).collect();
        pairs.sort();
        pairs
    });

    // ── FromIterator ────────────────────────────────────────────────
    support::task("Build a context from an iterator of pairs", || {
        let pairs = vec![
            ("fruit".to_string(), "apple".to_string()),
            ("vegetable".to_string(), "carrot".to_string()),
        ];
        let _ctx: Context = pairs.into_iter().collect();
    });

    // ── Hashing ─────────────────────────────────────────────────────
    support::task_with_output(
        "Hash stability across equal contexts",
        || {
            let mut a = Context::new();
            a.set("k".to_string(), "v".to_string());
            let mut b = Context::new();
            b.set("k".to_string(), "v".to_string());
            let mut c = b.clone();
            c.set("k2".to_string(), "v2".to_string());

            vec![
                format!("hash(a) == hash(b): {}", a.hash() == b.hash()),
                format!("hash(a) != hash(c): {}", a.hash() != c.hash()),
            ]
        },
    );

    support::summary(12);
}
