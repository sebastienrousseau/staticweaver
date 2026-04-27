// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(missing_docs)]

//! Criterion benches for the `staticweaver` template engine.
//!
//! Four scenarios:
//!   * `render_template`              - one tag, short template (baseline)
//!   * `render_template_escape_heavy` - 10 KiB value, 5% HTML metacharacters
//!   * `context_hash_100_keys`        - exercises `Context::hash`
//!   * `render_template_32_tags`      - exercises the scan loop

use criterion::{criterion_group, criterion_main, Criterion};
use staticweaver::{Context, Engine};
use std::hint::black_box;
use std::time::Duration;

/// The template string used for the baseline bench.
const TEMPLATE: &str = "<html><body>{{name}}</body></html>";

fn make_ctx_one() -> Context {
    let mut c = Context::new();
    c.set("name".to_string(), "Alice".to_string());
    c
}

fn benchmark_template_rendering(c: &mut Criterion) {
    let engine = Engine::new("dummy_path", Duration::from_secs(60));
    let _ = c.bench_function("render_template", |b| {
        b.iter_batched_ref(
            make_ctx_one,
            |ctx| {
                let _ = black_box(
                    engine
                        .render_template(
                            black_box(TEMPLATE),
                            black_box(ctx),
                        )
                        .expect("render"),
                );
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

/// 10 KiB payload with ~5% of bytes being HTML metacharacters.
fn benchmark_escape_heavy(c: &mut Criterion) {
    let engine = Engine::new("", Duration::from_secs(60));

    let body: String = (0..10_000)
        .map(|i| match i % 20 {
            0 => '<',
            1 => '>',
            2 => '&',
            _ => 'x',
        })
        .collect();

    let mut ctx = Context::new();
    ctx.set("body".to_string(), body);

    let _ = c.bench_function("render_template_escape_heavy", |b| {
        b.iter(|| {
            let _ = black_box(
                engine
                    .render_template(
                        black_box("<div>{{body}}</div>"),
                        black_box(&ctx),
                    )
                    .expect("render"),
            );
        });
    });
}

/// Context with 100 keys — dominates `render_page` cache-key construction.
fn benchmark_context_hash(c: &mut Criterion) {
    let mut ctx = Context::new();
    for i in 0..100 {
        ctx.set(format!("key{i:03}"), format!("value{i:03}"));
    }

    let _ = c.bench_function("context_hash_100_keys", |b| {
        b.iter(|| black_box(ctx.hash()));
    });
}

/// Template with 32 tags — stresses the delimiter-scan loop.
fn benchmark_many_tags(c: &mut Criterion) {
    let engine = Engine::new("", Duration::from_secs(60));

    let mut tmpl = String::new();
    for i in 0..32 {
        tmpl.push_str(&format!("{{{{k{i}}}}} "));
    }

    let mut ctx = Context::new();
    for i in 0..32 {
        ctx.set(format!("k{i}"), format!("v{i}"));
    }

    let _ = c.bench_function("render_template_32_tags", |b| {
        b.iter(|| {
            let _ = black_box(
                engine
                    .render_template(black_box(&tmpl), black_box(&ctx))
                    .expect("render"),
            );
        });
    });
}

criterion_group!(
    benches,
    benchmark_template_rendering,
    benchmark_escape_heavy,
    benchmark_context_hash,
    benchmark_many_tags,
);
criterion_main!(benches);
