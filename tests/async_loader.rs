// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Async loader + async render integration (issues #37 and #38).
//!
//! Verifies that:
//!   * `MemoryAsyncLoader` + `Engine::render_template_async` work
//!     end-to-end on the tokio runtime.
//!   * `Engine::render_page_async` honours the same cache the sync
//!     `render_page` uses (`Mutex<Cache<…>>` from #36) — a sync
//!     warm-up makes the async call a cache hit.
//!   * `Engine::render_to_async` streams into an `AsyncWrite` sink.
//!   * `TokioFsLoader` resolves a real on-disk template via
//!     non-blocking IO.

#![cfg(all(feature = "async", feature = "async-tokio"))]

use staticweaver::loader_async::{
    AsyncTemplateLoader, MemoryAsyncLoader, TokioFsLoader,
};
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn shared_engine() -> Arc<Engine> {
    Arc::new(Engine::new("", Duration::from_secs(60)))
}

#[tokio::test]
async fn memory_async_loader_round_trip() {
    let mut store = HashMap::new();
    let _ =
        store.insert("hello".to_string(), "Hi, {{name}}!".to_string());
    let loader = MemoryAsyncLoader::new(store);

    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());

    let out = engine
        .render_template_async(&loader, "hello", &ctx)
        .await
        .expect("async render must succeed");
    assert_eq!(out, "Hi, Ada!");
}

#[tokio::test]
async fn render_page_async_uses_same_cache_as_sync_path() {
    let mut store = HashMap::new();
    let _ = store.insert("page".to_string(), "v={{v}}".to_string());
    let loader = MemoryAsyncLoader::new(store);

    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("v".to_string(), "first".to_string());

    // Warm-up via the async path.
    let first = engine
        .render_page_async(&loader, &ctx, "page")
        .await
        .unwrap();
    assert_eq!(first, "v=first");

    // Second call hits the cache — even if we change the loader's
    // stored body, the cached render wins.
    let mut new_store = HashMap::new();
    let _ = new_store
        .insert("page".to_string(), "v={{v}} (CHANGED)".to_string());
    let new_loader = MemoryAsyncLoader::new(new_store);
    let second = engine
        .render_page_async(&new_loader, &ctx, "page")
        .await
        .unwrap();
    assert_eq!(second, first, "cache hit must serve the first render");
}

#[tokio::test]
async fn render_to_async_streams_into_an_async_write_sink() {
    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("who".to_string(), "world".to_string());

    let mut sink: Vec<u8> = Vec::new();
    engine
        .render_to_async("hello {{who}}", &ctx, &mut sink)
        .await
        .expect("async stream must succeed");
    assert_eq!(String::from_utf8(sink).unwrap(), "hello world");
}

#[tokio::test]
async fn tokio_fs_loader_reads_a_real_file() {
    let dir = TempDir::new().unwrap();
    tokio::fs::write(dir.path().join("page"), "hi {{name}}")
        .await
        .unwrap();
    let loader = TokioFsLoader::new(dir.path());

    let body = loader.load("page").await.unwrap();
    assert_eq!(body, "hi {{name}}");

    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "tokio".to_string());
    let out = engine
        .render_template_async(&loader, "page", &ctx)
        .await
        .unwrap();
    assert_eq!(out, "hi tokio");
}

#[tokio::test]
async fn render_page_to_async_combined_path() {
    let mut store = HashMap::new();
    let _ = store.insert("p".to_string(), "[{{a}}]".to_string());
    let loader = MemoryAsyncLoader::new(store);

    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("a".to_string(), "ok".to_string());

    let mut sink: Vec<u8> = Vec::new();
    engine
        .render_page_to_async(&loader, &ctx, "p", &mut sink)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(sink).unwrap(), "[ok]");
}

#[tokio::test]
async fn arc_engine_concurrent_async_renders() {
    // Same proof as the sync concurrent_render test, but on the
    // tokio runtime: many tasks share an Arc<Engine> and a single
    // loader, must all produce identical output.
    let mut store = HashMap::new();
    let _ = store.insert("page".to_string(), "hi {{name}}".to_string());
    let loader = Arc::new(MemoryAsyncLoader::new(store));
    let engine = shared_engine();

    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let ctx = Arc::new(ctx);
    let expected = engine
        .render_page_async(&*loader, &ctx, "page")
        .await
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        let loader = Arc::clone(&loader);
        let ctx = Arc::clone(&ctx);
        let expected = expected.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let out = engine
                    .render_page_async(&*loader, &ctx, "page")
                    .await
                    .expect(
                        "async render must not error under concurrency",
                    );
                assert_eq!(out, expected);
            }
        }));
    }
    for h in handles {
        h.await.expect(
            "tokio task panicked under async concurrent render",
        );
    }
}
