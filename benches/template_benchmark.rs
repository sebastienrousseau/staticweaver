// SPDX-License-Identifier: Apache-2.0 OR MIT
// See LICENSE-APACHE.md and LICENSE-MIT.md in the repository root for full license information.

#![allow(missing_docs)]

//! # Template Benchmark
//!
//! This benchmark measures the performance of the `staticweaver` template rendering engine.
//! It uses the `Criterion` library to run benchmarks and measure the time taken to render
//! templates using various contexts and cached templates.

use criterion::{
    black_box, criterion_group, criterion_main, Criterion,
};
use staticweaver::Engine;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Benchmarks the performance of the template rendering engine by rendering a template with different contexts.
fn benchmark_template_rendering(c: &mut Criterion) {
    // Create a temporary directory to simulate the template environment
    let temp_dir = Arc::new(Mutex::new(TempDir::new().unwrap()));
    let root_path = temp_dir.lock().unwrap().path().to_path_buf();

    // Create a template file
    let mut template_file =
        File::create(root_path.join("template.html")).unwrap();
    template_file
        .write_all(b"<html><body>{{ name }}</body></html>")
        .unwrap();

    // Initialize the engine
    let engine = Engine::new(
        root_path.to_str().unwrap(),
        Duration::from_secs(60),
    );

    // Benchmark the template rendering
    let _ = c.bench_function("template_rendering", |b| {
        b.iter_batched_ref(
            || {
                // Setup for each batch, create a fresh context
                let mut context = HashMap::new();
                let _ = context
                    .insert("name".to_string(), "Alice".to_string());
                context
            },
            |context| {
                // Render the template with the context
                let rendered = engine
                    .render_template("template.html", context)
                    .unwrap();
                let _ = black_box(rendered);
            },
            criterion::BatchSize::SmallInput, // Control batch size
        )
    });

    // Clean up
    drop(temp_dir);
}

// Criterion group and main function to set up and run the benchmark.
criterion_group!(benches, benchmark_template_rendering);
criterion_main!(benches);
