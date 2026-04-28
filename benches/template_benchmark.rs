// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(missing_docs)]

//! Criterion benches for the `staticweaver` template engine.
//!
//! Internal regression guards (the *comparative* matrix vs Tera /
//! Minijinja / Askama lives in `comparative.rs`):
//!
//!   * `render_template`              - one tag, short template (baseline)
//!   * `render_template_escape_heavy` - 10 KiB value, 5% HTML metacharacters
//!   * `context_hash_100_keys`        - exercises `Context::hash`
//!   * `render_template_32_tags`      - exercises the scan loop
//!   * `render_to_vs_render_template` - defends the streaming claim
//!   * `render_inheritance`           - {{#extends}} + {{#block}} + {{ super() }}
//!   * `render_partial_in_each`       - partial dispatch in a hot loop
//!   * `custom_filter_vs_builtin`     - dispatch-overhead regression guard

use criterion::{criterion_group, criterion_main, Criterion};
use staticweaver::engine::MemoryLoader;
use staticweaver::{Context, Engine};
use std::hint::black_box;
use std::sync::Arc;
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

/// Defends the streaming claim made by `Engine::render_to`. The
/// claim is *ergonomic* parity, not a perf win — this bench catches
/// any regression that would make the streaming path materially
/// slower than rendering to a `String` and copying the bytes.
fn benchmark_render_to_vs_render_template(c: &mut Criterion) {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    for i in 0..32 {
        ctx.set(format!("k{i}"), format!("value{i:05}"));
    }
    let mut tmpl = String::new();
    for i in 0..32 {
        tmpl.push_str(&format!("[{{{{k{i}}}}}]"));
    }

    let _ = c.bench_function("render_template_to_string", |b| {
        b.iter(|| {
            let _ = black_box(
                engine
                    .render_template(black_box(&tmpl), black_box(&ctx))
                    .expect("render"),
            );
        });
    });

    let _ = c.bench_function("render_to_vec", |b| {
        b.iter(|| {
            let mut buf: Vec<u8> = Vec::with_capacity(512);
            engine
                .render_to(black_box(&tmpl), black_box(&ctx), &mut buf)
                .expect("render_to");
            let _ = black_box(buf);
        });
    });
}

/// Exercises `{{#extends "base"}}` + `{{#block …}}` + `{{ super() }}`
/// — the inheritance chain pays for two template fetches, the block
/// merge map, and the parent-body re-render inside super(). Proves
/// the inheritance path doesn't blow up in cost.
fn benchmark_render_inheritance(c: &mut Criterion) {
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "<html><body>\n\
         <h1>{{#block \"title\"}}default{{/block}}</h1>\n\
         <main>{{#block \"body\"}}{{content}}{{/block}}</main>\n\
         </body></html>"
            .to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"title\"}}{{ super() }} :: {{title}}{{/block}}\
         {{#block \"body\"}}\
            <article>{{ super() }}</article>\
         {{/block}}"
            .to_string(),
    );
    let mut engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    // Disable cache so the bench measures real rendering, not a
    // hashmap hit. (render_page caches by (layout, ctx-hash).)
    engine.set_max_cache_size(0);
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "Hello".to_string());
    ctx.set(
        "content".to_string(),
        "Article body with <some> &chars; that need escape."
            .to_string(),
    );

    let _ = c.bench_function("render_inheritance_with_super", |b| {
        b.iter(|| {
            let _ = black_box(
                engine
                    .render_page(black_box(&ctx), black_box("child"))
                    .expect("render_page"),
            );
        });
    });
}

/// `#each` of N items where each iteration includes a partial. The
/// partial loader is hit N times — caching is on the rendered-page
/// level, not the partial-load level, so this is the realistic
/// hot-loop cost when each item drags in a partial.
fn benchmark_render_partial_in_each(c: &mut Criterion) {
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "row".to_string(),
        "<tr><td>{{this}}</td></tr>".to_string(),
    );
    let mut engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    engine.set_max_cache_size(0);
    let mut ctx = Context::new();
    let items: Vec<String> =
        (0..100).map(|i| format!("item-{i:03}")).collect();
    ctx.set_value("items".to_string(), items);
    let template = "<table>{{#each items}}{{> row}}{{/each}}</table>";

    let _ = c.bench_function("render_partial_in_each_100", |b| {
        b.iter(|| {
            let _ = black_box(
                engine
                    .render_template(
                        black_box(template),
                        black_box(&ctx),
                    )
                    .expect("render"),
            );
        });
    });
}

/// Custom-filter dispatch goes through an extra HashMap lookup before
/// falling through to the built-in chain. This bench proves the
/// override path is competitive — both branches do the same string
/// transformation, so any large gap is dispatch overhead.
fn benchmark_custom_filter_vs_builtin(c: &mut Criterion) {
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "ada lovelace".to_string());
    let template = "{{ name | uppercase }}";

    let builtin_engine = Engine::new("", Duration::from_secs(60));
    let _ =
        c.bench_function("filter_dispatch_builtin_uppercase", |b| {
            b.iter(|| {
                let _ = black_box(
                    builtin_engine
                        .render_template(
                            black_box(template),
                            black_box(&ctx),
                        )
                        .expect("render"),
                );
            });
        });

    let mut custom_engine = Engine::new("", Duration::from_secs(60));
    let _ = custom_engine.add_filter(
        "uppercase",
        Arc::new(|s, _args| Ok(s.to_uppercase())),
    );
    let _ = c.bench_function("filter_dispatch_custom_uppercase", |b| {
        b.iter(|| {
            let _ = black_box(
                custom_engine
                    .render_template(
                        black_box(template),
                        black_box(&ctx),
                    )
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
    benchmark_render_to_vs_render_template,
    benchmark_render_inheritance,
    benchmark_render_partial_in_each,
    benchmark_custom_filter_vs_builtin,
);
criterion_main!(benches);
