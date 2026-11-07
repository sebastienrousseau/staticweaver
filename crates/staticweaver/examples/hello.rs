// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Getting started: render a template string against a context.
//!
//! Run: `cargo run --example hello`

#[path = "support.rs"]
mod support;

use staticweaver::{Context, Engine};
use std::time::Duration;

fn main() {
    support::header("staticweaver -- hello");

    let engine =
        support::task("Build an Engine with a 60 s cache", || {
            Engine::new("templates", Duration::from_secs(60))
        });

    let context = support::task("Populate a Context", || {
        let mut ctx = Context::new();
        ctx.set("greeting".to_string(), "Hello".to_string());
        ctx.set("name".to_string(), "World".to_string());
        ctx
    });

    let rendered =
        support::task("Render `{{greeting}}, {{name}}!`", || {
            engine
                .render_template("{{greeting}}, {{name}}!", &context)
                .expect("render should succeed")
        });

    support::task_with_output("Inspect the output", || {
        vec![format!("rendered = {rendered:?}")]
    });

    support::summary(4);
}
