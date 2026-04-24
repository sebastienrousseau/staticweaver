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

    // ── File-backed render_page ─────────────────────────────────────
    let temp_dir = TempDir::new()?;
    let template_path = temp_dir.path().join("layout.html");
    fs::write(
        &template_path,
        "<html><body>{{!content}}</body></html>",
    )?;
    let mut page_engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut page_ctx = Context::new();
    page_ctx.set(
        "content".to_string(),
        "Welcome to StaticWeaver".to_string(),
    );

    let page = support::task("Render a page from disk", || {
        page_engine
            .render_page(&page_ctx, "layout")
            .expect("render_page should succeed")
    });
    support::task_with_output("Inspect the rendered page", || {
        vec![format!("page = {page}")]
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
    let mut cached_engine =
        Engine::new("templates", Duration::from_secs(60));
    let ctx = Context::new();
    support::task_with_output(
        "Cache fills on repeated renders",
        || {
            // render_template itself does not touch the page cache; use it to
            // show the render path, then populate the cache via render_page.
            let _ = cached_engine.render_template("static", &ctx);
            let _ = cached_engine.render_template("static", &ctx);
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

    support::summary(11);
    Ok(())
}
