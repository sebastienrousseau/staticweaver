// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Control flow + expression language showcase.
//!
//! Run: `cargo run --example control_flow`
//!
//! Demonstrates:
//!   * `{{#if EXPR}}` with comparisons, booleans, math, `~` concat,
//!     and `is X` postfix tests.
//!   * `{{#each list}}` with `@index`, `@first`, `@last`, `@key`.
//!   * `{{#each START..END}}` range form.
//!   * `{{#break}}` / `{{#continue}}` for loop control.
//!   * `{{#set k = v}}` for in-template assignment.

#[path = "support.rs"]
mod support;

use staticweaver::context::Value;
use staticweaver::{Context, Engine};
use std::time::Duration;

fn main() {
    support::header("staticweaver -- control flow");

    let engine = support::task("Build an Engine", || {
        Engine::new("", Duration::from_secs(60))
    });

    // ── Expression language ───────────────────────────────────────
    support::task_with_output(
        "Expressions: comparisons, booleans, math, concat",
        || {
            let mut ctx = Context::new();
            ctx.set("name".to_string(), "Ada".to_string());
            ctx.set_value("score".to_string(), 92i64);
            vec![
                engine
                    .render_template(
                        "{{#if score >= 90}}A{{else if score >= 70}}B{{else}}C{{/if}}",
                        &ctx,
                    )
                    .unwrap_or_else(|_| {
                        // staticweaver doesn't have `else if` yet — show
                        // the closest equivalent with nested if.
                        engine
                            .render_template(
                                "{{#if score >= 90}}A{{else}}{{#if score >= 70}}B{{else}}C{{/if}}{{/if}}",
                                &ctx,
                            )
                            .unwrap()
                    }),
                engine
                    .render_template(
                        r#"{{#if name ~ " is back" == "Ada is back"}}match{{else}}no{{/if}}"#,
                        &ctx,
                    )
                    .unwrap(),
                engine
                    .render_template(
                        "{{#if score > 50 and score < 100}}in range{{else}}out{{/if}}",
                        &ctx,
                    )
                    .unwrap(),
            ]
        },
    );

    // ── `is X` postfix tests ──────────────────────────────────────
    support::task_with_output(
        "Postfix tests: is defined / is empty / is none",
        || {
            let mut ctx = Context::new();
            ctx.set("present".to_string(), "value".to_string());
            ctx.set_value("blank".to_string(), Value::Null);
            vec![
                engine
                    .render_template(
                        "{{#if present is defined}}Y{{else}}N{{/if}}",
                        &ctx,
                    )
                    .unwrap(),
                engine
                    .render_template(
                        "{{#if missing is not defined}}Y{{else}}N{{/if}}",
                        &ctx,
                    )
                    .unwrap(),
                engine
                    .render_template(
                        "{{#if blank is none}}null{{else}}set{{/if}}",
                        &ctx,
                    )
                    .unwrap(),
            ]
        },
    );

    // ── #each over List, with helpers ─────────────────────────────
    support::task_with_output(
        "#each over List with @index / @first / @last",
        || {
            let mut ctx = Context::new();
            ctx.set_value(
                "items".to_string(),
                vec!["alpha", "beta", "gamma"],
            );
            vec![engine
                .render_template(
                    "{{#each items}}\
                     [{{@index}}={{this}}{{#if @first}} (first){{/if}}{{#if @last}} (last){{/if}}]\
                     {{/each}}",
                    &ctx,
                )
                .unwrap()]
        },
    );

    // ── #each over a range ────────────────────────────────────────
    support::task_with_output("#each over a range (1..6)", || {
        let ctx = Context::new();
        vec![engine
                .render_template(
                    "{{#each 1..6}}{{this}}{{#if @last}}{{else}}, {{/if}}{{/each}}",
                    &ctx,
                )
                .unwrap()]
    });

    // ── Loop control: #break / #continue ──────────────────────────
    support::task_with_output(
        "Loop control: #break and #continue",
        || {
            let mut ctx = Context::new();
            ctx.set_value(
                "items".to_string(),
                vec!["a", "skip", "b", "stop", "c"],
            );
            let with_continue = engine
                .render_template(
                    "{{#each items}}\
                     {{#if this == \"skip\"}}{{#continue}}{{/if}}\
                     [{{this}}]\
                     {{/each}}",
                    &ctx,
                )
                .unwrap();
            let with_break = engine
                .render_template(
                    "{{#each items}}\
                     {{#if this == \"stop\"}}{{#break}}{{/if}}\
                     [{{this}}]\
                     {{/each}}",
                    &ctx,
                )
                .unwrap();
            vec![
                format!("continue: {with_continue}"),
                format!("break:    {with_break}"),
            ]
        },
    );

    // ── In-template assignment ────────────────────────────────────
    support::task_with_output("In-template `#set`", || {
        let ctx = Context::new();
        vec![engine
            .render_template(
                "{{#set greeting = \"Hello\"}}\
                 {{#set who = \"world\"}}\
                 {{ greeting }}, {{ who }}!",
                &ctx,
            )
            .unwrap()]
    });

    support::summary(7);
}
