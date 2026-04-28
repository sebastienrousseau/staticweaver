// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Pluggable template loaders: `FsLoader`, `MemoryLoader`, and a
//! custom backend via the `TemplateLoader` trait.
//!
//! Run: `cargo run --example loaders`
//!
//! The `TemplateLoader` trait is `Send + Sync`, so any loader you
//! implement can be wrapped in `Arc` and passed to
//! `Engine::with_loader`. Useful for embedded asset bundles,
//! database-backed CMSs, or test harnesses that don't want to
//! touch the filesystem.

#[path = "support.rs"]
mod support;

use staticweaver::engine::{FsLoader, MemoryLoader, TemplateLoader};
use staticweaver::{Context, Engine, EngineError};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Toy custom loader: serves templates from a thread-safe map
/// that other parts of the program can mutate at runtime
/// (without rebuilding the engine).
#[derive(Default)]
struct LiveLoader {
    store: RwLock<HashMap<String, String>>,
}

impl TemplateLoader for LiveLoader {
    fn load(&self, name: &str) -> Result<String, EngineError> {
        self.store
            .read()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| {
                EngineError::Render(format!(
                    "LiveLoader: no template `{name}`"
                ))
            })
    }
}

fn main() {
    support::header("staticweaver -- loaders");

    // ── FsLoader (default backend) ────────────────────────────────
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("page.html"),
        "Hello, {{ name }}! (FsLoader)",
    )
    .unwrap();

    support::task_with_output("FsLoader (read from disk)", || {
        let mut engine = Engine::with_loader(
            Arc::new(FsLoader::new(temp.path().to_path_buf())),
            Duration::from_secs(60),
        );
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        vec![engine.render_page(&ctx, "page").unwrap()]
    });

    // ── MemoryLoader (built-in in-memory backend) ─────────────────
    support::task_with_output(
        "MemoryLoader (in-memory; useful for tests / embedded assets)",
        || {
            let mut store = HashMap::new();
            let _ = store.insert(
                "page".to_string(),
                "Hello, {{ name }}! (MemoryLoader)".to_string(),
            );
            let mut engine = Engine::with_loader(
                Arc::new(MemoryLoader::new(store)),
                Duration::from_secs(60),
            );
            let mut ctx = Context::new();
            ctx.set("name".to_string(), "Alan".to_string());
            vec![engine.render_page(&ctx, "page").unwrap()]
        },
    );

    // ── Custom LiveLoader implementing TemplateLoader ─────────────
    support::task_with_output(
        "Custom loader: hot-mutable RwLock<HashMap>",
        || {
            let live = Arc::new(LiveLoader::default());
            // Mutate the loader at runtime from anywhere — the
            // engine sees the change immediately.
            let _ = live.store.write().unwrap().insert(
                "live".to_string(),
                "live update {{ count }}".to_string(),
            );

            let mut engine = Engine::with_loader(
                live.clone(),
                Duration::from_secs(60),
            );
            let mut ctx = Context::new();
            ctx.set_value("count".to_string(), 1i64);
            let first = engine.render_page(&ctx, "live").unwrap();

            // Hot-swap the body, render again — picks up the change.
            let _ = live.store.write().unwrap().insert(
                "live".to_string(),
                "live update v2: {{ count }}".to_string(),
            );
            ctx.set_value("count".to_string(), 2i64);
            // Force a cache miss for the new body.
            engine.clear_cache();
            let second = engine.render_page(&ctx, "live").unwrap();

            vec![first, second]
        },
    );

    support::summary(3);
}
