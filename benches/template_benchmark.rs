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

/// Benchmarks the performance of the template rendering engine by rendering a template with different contexts.
fn benchmark_template_rendering(c: &mut Criterion) {
    // Initialize the engine
    let engine = Engine::new("dummy_path", Duration::from_secs(60));

    // Create a template string
    let template = "<html><body>{{name}}</body></html>";

    // Benchmark the template rendering
    let _ = c.bench_function("template_rendering", |b| {
        b.iter_batched_ref(
            || {
                // Setup for each batch, create a fresh context
                let mut context = Context::new();
                context.set("name".to_string(), "Alice".to_string());
                context
            },
            |context| {
                // Render the template with the context
                let rendered = engine
                    .render_template(
                        black_box(template),
                        black_box(context),
                    )
                    .expect("Failed to render template");
                let _ = black_box(rendered);
            },
            criterion::BatchSize::SmallInput, // Control batch size
        );
    });
}

// Criterion group and main function to set up and run the benchmark.
criterion_group!(benches, benchmark_template_rendering);
criterion_main!(benches);
