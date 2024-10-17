// Copyright © 2024 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(missing_docs)]

//! # Template Benchmark
//!
//! This benchmark measures the performance of the `staticweaver` template rendering engine.
//! It uses the `Criterion` library to run benchmarks and measure the time taken to render
//! templates using various contexts and cached templates.

use criterion::{
    black_box, criterion_group, criterion_main, Criterion,
};
use staticweaver::{Context, Engine};
use std::time::Duration;

/// The template string used for benchmarking.
const TEMPLATE: &str = "<html><body>{{name}}</body></html>";

/// Creates a context for benchmarking.
fn create_benchmark_context() -> Context {
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    context
}

/// Renders a template for benchmarking.
fn render_template(
    engine: &Engine,
    template: &str,
    context: &Context,
) -> String {
    engine
        .render_template(black_box(template), black_box(context))
        .expect("Failed to render template")
}

/// Benchmarks the performance of the template rendering engine by rendering a template with different contexts.
fn benchmark_template_rendering(c: &mut Criterion) {
    let engine = Engine::new("dummy_path", Duration::from_secs(60));

    let _ = c.bench_function("template_rendering", |b| {
        b.iter_batched_ref(
            create_benchmark_context,
            |context| {
                let _ = black_box(render_template(
                    &engine, TEMPLATE, context,
                ));
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, benchmark_template_rendering);
criterion_main!(benches);
