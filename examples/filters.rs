// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Built-in + custom filters and tests.
//!
//! Run: `cargo run --example filters`
//!
//! Showcases the 23 built-in filters, then registers a custom
//! `slugify` filter via `Engine::add_filter` and a custom `vip` test
//! via `Engine::add_test`. Custom override built-ins of the same
//! name (e.g. you can replace `uppercase` with a locale-aware
//! implementation if you need to).

#[path = "support.rs"]
mod support;

use staticweaver::context::Value;
use staticweaver::{Context, Engine};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    support::header("staticweaver -- filters");

    let engine = support::task("Build an Engine", || {
        let mut e = Engine::new("", Duration::from_secs(60));
        // A custom filter — receives &str + colon-separated args.
        let _ = e.add_filter(
            "slugify",
            Arc::new(|input, _args| {
                Ok(input
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect())
            }),
        );
        // A custom test — receives the operand Value + args; returns bool.
        let _ = e.add_test(
            "vip",
            Arc::new(|v, _args| {
                Ok(matches!(v, Value::String(s) if s == "ada" || s == "alan"))
            }),
        );
        e
    });

    let mut ctx = Context::new();
    ctx.set("title".to_string(), "  staticweaver  ".to_string());
    ctx.set(
        "bio".to_string(),
        "A small templating engine for Rust.".to_string(),
    );
    ctx.set_value("count".to_string(), 1_234_567i64);
    ctx.set("name".to_string(), "ada".to_string());

    support::task_with_output(
        "Built-in filters: trim | uppercase",
        || {
            vec![engine
                .render_template("{{ title | trim | uppercase }}", &ctx)
                .unwrap()]
        },
    );

    support::task_with_output(
        "Built-in filters: truncate (default 30) + capitalize",
        || {
            vec![engine
                .render_template(
                    "{{ bio | truncate | capitalize }}",
                    &ctx,
                )
                .unwrap()]
        },
    );

    support::task_with_output(
        "Built-in number filters: number_format",
        || {
            vec![engine
                .render_template("{{ count | number_format }}", &ctx)
                .unwrap()]
        },
    );

    support::task_with_output(
        "Built-in string filters: pad_start, repeat, slice",
        || {
            vec![
                engine
                    .render_template("{{ name | pad_start:8 }}", &ctx)
                    .unwrap(),
                engine
                    .render_template("{{ name | repeat:3 }}", &ctx)
                    .unwrap(),
                engine
                    .render_template("{{ name | slice:0,2 }}", &ctx)
                    .unwrap(),
            ]
        },
    );

    support::task_with_output("Custom filter: slugify", || {
        let mut local = ctx.clone();
        local.set("raw_title".to_string(), "Hello, World!".to_string());
        vec![engine
            .render_template("{{ raw_title | slugify }}", &local)
            .unwrap()]
    });

    support::task_with_output("Custom test: `is vip`", || {
        let yes = engine
            .render_template(
                "{{#if name is vip}}YES{{else}}NO{{/if}}",
                &ctx,
            )
            .unwrap();
        let mut other = ctx.clone();
        other.set("name".to_string(), "carol".to_string());
        let no = engine
            .render_template(
                "{{#if name is vip}}YES{{else}}NO{{/if}}",
                &other,
            )
            .unwrap();
        vec![format!("ada -> {yes}"), format!("carol -> {no}")]
    });

    support::summary(7);
}
