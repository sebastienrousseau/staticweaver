// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Async render via Tokio (issues #37 + #38).
//!
//! Demonstrates:
//!   * `MemoryAsyncLoader` for in-memory async templates
//!   * `Engine::render_template_async` and `render_page_async`
//!   * `Engine::render_to_async` streaming into an `AsyncWrite` sink
//!   * Sharing one `Arc<Engine>` across multiple tokio tasks
//!
//! Run with: `cargo run --example async_tokio --features async-tokio`

#[path = "support.rs"]
mod support;

use staticweaver::loader_async::MemoryAsyncLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    support::header("staticweaver -- async tokio");

    let mut store = HashMap::new();
    let _ = store.insert(
        "greeting".to_string(),
        "Hello, {{name}} (#{{id}})!".to_string(),
    );
    let loader = Arc::new(MemoryAsyncLoader::new(store));

    let engine = Arc::new(Engine::new("", Duration::from_secs(60)));

    // ── render_template_async ───────────────────────────────────────
    {
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Ada".to_string());
        ctx.set_value("id".to_string(), 1i64);

        let out = engine
            .render_template_async(&*loader, "greeting", &ctx)
            .await?;
        support::task_with_output("Async template render", || {
            vec![format!("rendered = {out}")]
        });
    }

    // ── render_page_async (caches in the same Mutex<Cache> the sync
    //     path uses) ───────────────────────────────────────────────
    {
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Grace".to_string());
        ctx.set_value("id".to_string(), 2i64);

        let _ = engine
            .render_page_async(&*loader, &ctx, "greeting")
            .await?;
        let stats = engine.render_cache.lock().unwrap().stats();
        support::task_with_output("Cache after async warm-up", || {
            vec![format!("inserts = {}", stats.inserts)]
        });
    }

    // ── render_to_async streams into a Vec<u8> ─────────────────────
    {
        let mut ctx = Context::new();
        ctx.set("name".to_string(), "Linus".to_string());
        ctx.set_value("id".to_string(), 3i64);

        let mut sink: Vec<u8> = Vec::new();
        engine
            .render_to_async("hi {{name}}!", &ctx, &mut sink)
            .await?;
        support::task_with_output(
            "Stream into an AsyncWrite sink",
            || {
                vec![format!(
                    "buf = {:?}",
                    String::from_utf8(sink).unwrap()
                )]
            },
        );
    }

    // ── Fan out across tokio tasks ─────────────────────────────────
    {
        let mut handles = Vec::new();
        for id in 0..4 {
            let engine = Arc::clone(&engine);
            let loader = Arc::clone(&loader);
            handles.push(tokio::spawn(async move {
                let mut ctx = Context::new();
                ctx.set("name".to_string(), format!("worker-{id}"));
                ctx.set_value("id".to_string(), id as i64);
                engine
                    .render_page_async(&*loader, &ctx, "greeting")
                    .await
                    .unwrap()
            }));
        }
        let mut outs = Vec::new();
        for h in handles {
            outs.push(h.await?);
        }
        support::task_with_output(
            "Fan-out across 4 tokio tasks",
            || outs.iter().map(|s| format!("  {s}")).collect(),
        );
    }

    Ok(())
}
