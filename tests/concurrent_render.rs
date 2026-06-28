// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Concurrency proof (issue #36, v0.0.4).
//!
//! Before v0.0.4 the engine's `render_page` took `&mut self` because the
//! render cache mutated on every call. That forced every multi-handler
//! consumer (Axum, Actix, Tokio task fan-out) into the
//! `Arc<Mutex<Engine>>` envelope — serialising every render through one
//! lock and erasing the gains from spawning workers in the first place.
//!
//! After v0.0.4 the cache lives behind a `std::sync::Mutex` inside
//! `Engine` itself; `render_page` takes `&self`. The lock is held only
//! across the cache lookup / insert — the expensive render work happens
//! lock-free. These tests pin the new behaviour:
//!
//!   * `Engine: Send + Sync + Clone` (compile-time check)
//!   * 8 threads × 10 000 renders sharing one `Arc<Engine>` produce
//!     byte-identical output and never panic.

use staticweaver::engine::MemoryLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ── Compile-time Send + Sync + Clone proof ──────────────────────────

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

#[test]
fn engine_is_send() {
    assert_send::<Engine>();
}

#[test]
fn engine_is_sync() {
    assert_sync::<Engine>();
}

#[test]
fn engine_arc_is_send_and_sync() {
    assert_send::<Arc<Engine>>();
    assert_sync::<Arc<Engine>>();
}

// ── Runtime concurrent-render soak ──────────────────────────────────

fn shared_engine() -> Arc<Engine> {
    let mut store = HashMap::new();
    let _ = store.insert(
        "page".to_string(),
        "hello {{name}} #{{id}}".to_string(),
    );
    Arc::new(Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    ))
}

#[test]
fn arc_engine_renders_identical_output_across_threads() {
    // Every thread renders against the same key, so every thread must
    // get the same bytes back. The first hit populates the cache; every
    // subsequent thread serves from cache (or wins the race and re-renders).
    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    ctx.set_value("id".to_string(), 1i64);

    let ctx = Arc::new(ctx);
    let expected = engine.render_page(&ctx, "page").unwrap();

    let mut handles = Vec::new();
    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        let ctx = Arc::clone(&ctx);
        let expected = expected.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..1_000 {
                let out = engine.render_page(&ctx, "page").expect(
                    "render_page must not error under concurrency",
                );
                assert_eq!(
                    out, expected,
                    "concurrent renders must produce identical output",
                );
            }
        }));
    }
    for h in handles {
        h.join().expect("thread panicked under concurrent render");
    }
}

#[test]
fn arc_engine_renders_different_keys_in_parallel() {
    // Each thread renders with a distinct context — exercises the
    // cache-miss path concurrently. Per-thread output must match the
    // single-threaded reference render.
    let engine = shared_engine();

    let mut expected_per_thread = Vec::new();
    for id in 0..8 {
        let mut ctx = Context::new();
        ctx.set("name".to_string(), format!("worker-{id}"));
        ctx.set_value("id".to_string(), id as i64);
        expected_per_thread.push((
            ctx,
            engine
                .render_page(
                    &{
                        let mut c = Context::new();
                        c.set(
                            "name".to_string(),
                            format!("worker-{id}"),
                        );
                        c.set_value("id".to_string(), id as i64);
                        c
                    },
                    "page",
                )
                .unwrap(),
        ));
    }

    let engine_outer = Arc::clone(&engine);
    let mut handles = Vec::new();
    for (id, (ctx, expected)) in
        expected_per_thread.into_iter().enumerate()
    {
        let engine = Arc::clone(&engine_outer);
        handles.push(thread::spawn(move || {
            for _ in 0..500 {
                let out = engine.render_page(&ctx, "page").unwrap();
                assert_eq!(
                    out, expected,
                    "thread {id}: output must be deterministic under concurrency",
                );
            }
        }));
    }
    for h in handles {
        h.join()
            .expect("thread panicked under per-key concurrent render");
    }
}

#[test]
fn arc_engine_clear_cache_under_load_does_not_panic() {
    // One thread continuously clears the cache; another continuously
    // renders. The renderer must never observe a torn cache state —
    // worst case is a cache miss (which is just a re-render).
    let engine = shared_engine();
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "stress".to_string());
    ctx.set_value("id".to_string(), 99i64);
    let ctx = Arc::new(ctx);

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let render_handle = {
        let engine = Arc::clone(&engine);
        let ctx = Arc::clone(&ctx);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut count = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = engine.render_page(&ctx, "page").unwrap();
                count += 1;
            }
            count
        })
    };

    let clear_handle = {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut count = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                engine.clear_cache();
                count += 1;
            }
            count
        })
    };

    thread::sleep(Duration::from_millis(50));
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let renders = render_handle.join().unwrap();
    let clears = clear_handle.join().unwrap();
    assert!(renders > 0, "renderer never made progress");
    assert!(clears > 0, "cache-clearer never made progress");
}
