// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Template inheritance: `{{#extends}}` + `{{#block}}` with `{{ super() }}`.
//!
//! Run: `cargo run --example inheritance`
//!
//! Demonstrates a layered design: a base layout defines named blocks,
//! a child template overrides them — and inside the override, the
//! child can splice the parent's body back in via `{{ super() }}`.
//! Same shape as Jinja / Tera / Liquid.

#[path = "support.rs"]
mod support;

use staticweaver::engine::MemoryLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    support::header("staticweaver -- inheritance");

    let engine = support::task(
        "Build an Engine with an in-memory loader",
        || {
            let mut store = HashMap::new();
            let _ = store.insert(
                "base.html".to_string(),
                "\
<!doctype html>
<html><body>
  <h1>{{#block \"title\"}}staticweaver{{/block}}</h1>
  <main>{{#block \"body\"}}default body{{/block}}</main>
  <footer>{{#block \"footer\"}}c 2026{{/block}}</footer>
</body></html>
"
                .to_string(),
            );
            let _ = store.insert(
                "post.html".to_string(),
                "\
{{#extends \"base.html\"}}\
{{#block \"title\"}}{{ super() }} - {{ title }}{{/block}}\
{{#block \"body\"}}\
<article>
<h2>{{ title }}</h2>
<p>{{ body }}</p>
</article>\
{{/block}}\
"
                .to_string(),
            );
            Engine::with_loader(
                Arc::new(MemoryLoader::new(store)),
                Duration::from_secs(60),
            )
        },
    );

    let mut ctx = Context::new();
    ctx.set("title".to_string(), "Hello, inheritance!".to_string());
    ctx.set(
        "body".to_string(),
        "Child override + super() splice the parent body back in."
            .to_string(),
    );

    support::task_with_output(
        "Render `post.html` (extends base, overrides title + body)",
        || {
            let mut e = engine;
            let out =
                e.render_page(&ctx, "post.html").expect("render_page");
            out.lines().map(str::to_string).collect::<Vec<_>>()
        },
    );

    support::summary(2);
}
