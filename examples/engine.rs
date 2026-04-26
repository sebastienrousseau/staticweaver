// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! `Engine` operations: string templates, page files, delimiters, caching.
//!
//! Run: `cargo run --example engine`

#[path = "support.rs"]
mod support;

use staticweaver::{Context, Engine, EngineError};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    support::header("staticweaver -- engine");

    // ── Basic string template ───────────────────────────────────────
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("greeting".to_string(), "Hello".to_string());
    context.set("name".to_string(), "Alice".to_string());

    let rendered =
        support::task("Render `{{greeting}}, {{name}}!`", || {
            engine
                .render_template("{{greeting}}, {{name}}!", &context)
                .expect("render should succeed")
        });
    support::task_with_output("Inspect the rendered output", || {
        vec![format!("rendered = {rendered:?}")]
    });

    // ── Unresolved tag surfaces a Render error ──────────────────────
    support::task_with_output(
        "Unresolved tag returns Render error",
        || match engine
            .render_template("{{greeting}}, {{unresolved}}!", &context)
        {
            Err(EngineError::Render(msg)) => {
                vec![format!("caught = {msg}")]
            }
            other => {
                vec![format!("BUG: expected Render, got {other:?}")]
            }
        },
    );

    // ── Auto HTML escaping + raw opt-out ────────────────────────────
    let mut html_ctx = Context::new();
    html_ctx.set(
        "user".to_string(),
        "<script>alert('x')</script>".to_string(),
    );
    html_ctx.set("body".to_string(), "<b>hi</b>".to_string());

    support::task_with_output(
        "Escape user-provided values by default",
        || {
            let out = engine
                .render_template("Hello, {{user}}", &html_ctx)
                .expect("render");
            vec![format!("escaped = {out}")]
        },
    );

    support::task_with_output("Opt out with `{{!key}}`", || {
        let out = engine
            .render_template("body = {{!body}}", &html_ctx)
            .expect("render");
        vec![format!("raw = {out}")]
    });

    // ── Template Partials ───────────────────────────────────────────
    let partial_dir = TempDir::new()?;
    let header_path = partial_dir.path().join("header.html");
    fs::write(&header_path, "Welcome, {{name}}!")?;

    let partial_engine = Engine::new(
        partial_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut partial_ctx = Context::new();
    partial_ctx.set("name".to_string(), "Alice".to_string());

    let partial_out = support::task("Render with a partial", || {
        partial_engine
            .render_template("Header: {{> header}}", &partial_ctx)
            .expect("render should succeed")
    });
    support::task_with_output("Inspect the partial output", || {
        vec![format!("partial = {partial_out:?}")]
    });

    // ── Built-in Filters ────────────────────────────────────────────
    let mut filter_ctx = Context::new();
    filter_ctx.set("title".to_string(), " staticweaver ".to_string());
    filter_ctx.set(
        "long_text".to_string(),
        "This is a very long string that will be truncated".to_string(),
    );

    support::task_with_output(
        "Apply filters: trim | uppercase",
        || {
            let out = engine
                .render_template(
                    "Title: {{ title | trim | uppercase }}",
                    &filter_ctx,
                )
                .expect("render");
            vec![format!("filtered = {out:?}")]
        },
    );

    support::task_with_output("Apply filters: truncate", || {
        let out = engine
            .render_template(
                "Text: {{ long_text | truncate }}",
                &filter_ctx,
            )
            .expect("render");
        vec![format!("truncated = {out:?}")]
    });

    // ── Control Flow & Nested Data ──────────────────────────────────
    let mut flow_ctx = Context::new();
    flow_ctx.set_value("show_it".to_string(), true);
    flow_ctx.set_value("items".to_string(), vec!["apple", "banana"]);
    flow_ctx.set_value("user".to_string(), {
        let mut m = fnv::FnvHashMap::default();
        let _ = m.insert(
            "name".to_string(),
            staticweaver::context::Value::from("Ada"),
        );
        staticweaver::context::Value::Map(m)
    });

    support::task_with_output(
        "Render nested data: `{{user.name}}`",
        || {
            let out = engine
                .render_template("Hello, {{user.name}}!", &flow_ctx)
                .expect("render");
            vec![format!("dot_notation = {out:?}")]
        },
    );

    support::task_with_output(
        "Control flow: `{{#if}}`, `{{#each}}`",
        || {
            let template = "
{{#if show_it}}
List:
{{#each items}} - {{this}}
{{/each}}
{{else}}
Hidden
{{/if}}";
            let out = engine
                .render_template(template, &flow_ctx)
                .expect("render");
            vec!["result:".to_string(), out.trim().to_string()]
        },
    );

    // ── File-backed render_page ─────────────────────────────────────
    let temp_dir = TempDir::new()?;
    let layout_dir = temp_dir.path().join("blog");
    fs::create_dir_all(&layout_dir)?;
    let template_path = layout_dir.join("post.html");
    fs::write(&template_path, "<h1>{{title}}</h1>")?;
    let mut page_engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut page_ctx = Context::new();
    page_ctx
        .set("title".to_string(), "Subdirectory support".to_string());

    let page =
        support::task("Render a page from a subdirectory", || {
            page_engine
                .render_page(&page_ctx, "blog/post")
                .expect("render_page should succeed")
        });
    support::task_with_output("Inspect the rendered page", || {
        vec![format!("page = {page}")]
    });

    // ── HTML Escape Toggle ──────────────────────────────────────────
    let raw_engine = Engine::new("", Duration::from_secs(60))
        .with_html_escape(false);
    support::task_with_output("Disable HTML escaping globally", || {
        let out = raw_engine
            .render_template("User: {{user}}", &html_ctx)
            .expect("render");
        vec![format!("raw_global = {out}")]
    });

    // ── Path traversal is rejected ──────────────────────────────────
    support::task_with_output("`render_page` rejects `../foo`", || {
        match page_engine.render_page(&page_ctx, "../foo") {
            Err(EngineError::InvalidTemplate(msg)) => {
                vec![format!("caught = {msg}")]
            }
            other => vec![format!(
                "BUG: expected InvalidTemplate, got {other:?}"
            )],
        }
    });

    // ── Custom delimiters ───────────────────────────────────────────
    let mut delim_engine =
        Engine::new("templates", Duration::from_secs(60));
    delim_engine.set_delimiters("<<", ">>");
    let mut delim_ctx = Context::new();
    delim_ctx.set("name".to_string(), "Bob".to_string());
    let custom =
        support::task("Render with `<<`, `>>` delimiters", || {
            delim_engine
                .render_template("Hello, <<name>>!", &delim_ctx)
                .expect("render")
        });
    support::task_with_output("Inspect the output", || {
        vec![format!("custom_delim = {custom:?}")]
    });

    // ── Cache management ────────────────────────────────────────────
    let mut cached_engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    support::task_with_output(
        "Cache fills on repeated renders",
        || {
            // render_page populates the cache
            let _ = cached_engine.render_page(&page_ctx, "blog/post");
            let _ = cached_engine.render_page(&page_ctx, "blog/post");
            vec![format!(
                "render_cache.len = {}",
                cached_engine.render_cache.len()
            )]
        },
    );
    support::task_with_output(
        "`clear_cache` empties the cache",
        || {
            cached_engine.clear_cache();
            vec![format!(
                "render_cache.len = {}",
                cached_engine.render_cache.len()
            )]
        },
    );

    support::summary(18);
    Ok(())
}
