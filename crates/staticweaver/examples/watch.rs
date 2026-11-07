// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Re-render when a template changes, with `WatchLoader` (issue #52).
//!
//! Writes a template to a temporary directory, renders it, edits the
//! file, waits for the debounced change batch, and renders again.
//!
//! Run: `cargo run --example watch --features watch`

#[path = "support.rs"]
mod support;

use staticweaver::engine::FsLoader;
use staticweaver::watch::WatchLoader;
use staticweaver::{Context, Engine};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    support::header("staticweaver -- watch");

    let dir = tempfile::tempdir().expect("temp dir");
    let page = dir.path().join("index.html");
    std::fs::write(&page, "<h1>{{title}}</h1>")
        .expect("write template");

    let loader =
        support::task("Wrap an FsLoader in a WatchLoader", || {
            Arc::new(
                WatchLoader::new(
                    FsLoader::new(dir.path().into()),
                    dir.path(),
                )
                .expect("watch the template directory"),
            )
        });
    let changes = loader.subscribe();
    let engine =
        Engine::with_loader(loader.clone(), Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "Hello".to_string());

    let first = support::task("Render index", || {
        engine.render_page(&ctx, "index").expect("render")
    });

    support::task("Edit index.html", || {
        std::fs::write(&page, "<h1>{{title}}, again</h1>")
            .expect("rewrite");
    });

    let batch = support::task("Wait for the debounced change", || {
        changes
            .recv_timeout(Duration::from_secs(10))
            .expect("a change batch within 10 s")
    });

    let second =
        support::task("Clear the render cache and re-render", || {
            engine.clear_cache();
            engine.render_page(&ctx, "index").expect("render")
        });

    support::task_with_output("Inspect the output", || {
        vec![
            format!("before = {first:?}"),
            format!("changed paths = {}", batch.len()),
            format!("after = {second:?}"),
            format!("loader generation = {}", loader.generation()),
        ]
    });

    support::summary(6);
}
