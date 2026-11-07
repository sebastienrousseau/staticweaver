// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Error handling: every `EngineError` / `TemplateError` variant.
//!
//! Run: `cargo run --example errors`

#[path = "support.rs"]
mod support;

use staticweaver::{Context, Engine, EngineError, TemplateError};
use std::fs::{self, File};
use std::io;
use std::time::Duration;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    support::header("staticweaver -- errors");

    // ── Io: missing template file ───────────────────────────────────
    let temp_dir = TempDir::new()?;
    let engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    support::task_with_output(
        "render_page on a missing file -> Io error",
        || match engine.render_page(&ctx, "missing") {
            Err(EngineError::Io(e)) => {
                vec![format!("kind = {:?}", e.kind())]
            }
            other => vec![format!("BUG: expected Io, got {other:?}")],
        },
    );

    // ── InvalidTemplate: traversal rejected before any IO ───────────
    support::task_with_output(
        "render_page `../etc/passwd` -> InvalidTemplate",
        || match engine.render_page(&ctx, "../etc/passwd") {
            Err(EngineError::InvalidTemplate(msg)) => {
                vec![format!("message = {msg}")]
            }
            other => vec![format!(
                "BUG: expected InvalidTemplate, got {other:?}"
            )],
        },
    );

    // ── InvalidTemplate: unclosed tag ───────────────────────────────
    let engine2 = Engine::new("templates", Duration::from_secs(60));
    support::task_with_output(
        "Unclosed tag -> InvalidTemplate",
        || match engine2.render_template("Hello, {{name", &ctx) {
            Err(EngineError::InvalidTemplate(msg)) => {
                vec![format!("message = {msg}")]
            }
            other => vec![format!(
                "BUG: expected InvalidTemplate, got {other:?}"
            )],
        },
    );

    // ── InvalidTemplate: nested delimiter ───────────────────────────
    support::task_with_output(
        "Nested `{{ {{ }} }}` -> InvalidTemplate",
        || match engine2.render_template("{{outer{{inner}}}}", &ctx) {
            Err(EngineError::InvalidTemplate(msg)) => {
                vec![format!("message = {msg}")]
            }
            other => vec![format!(
                "BUG: expected InvalidTemplate, got {other:?}"
            )],
        },
    );

    // ── Render: unresolved context key ──────────────────────────────
    support::task_with_output(
        "Unresolved `{{name}}` -> Render error",
        || match engine2.render_template("Hi {{name}}", &ctx) {
            Err(EngineError::Render(msg)) => {
                vec![format!("message = {msg}")]
            }
            other => {
                vec![format!("BUG: expected Render, got {other:?}")]
            }
        },
    );

    // ── Successful render after a populated context ─────────────────
    let layout = temp_dir.path().join("ok.html");
    fs::write(&layout, "Hi {{name}}")?;
    let mut ctx_ok = Context::new();
    ctx_ok.set("name".to_string(), "Alice".to_string());
    let ok =
        support::task("render_page recovers with a valid key", || {
            engine
                .render_page(&ctx_ok, "ok")
                .expect("render_page should succeed")
        });
    support::task_with_output("Inspect success output", || {
        vec![format!("rendered = {ok}")]
    });

    // ── TemplateError direct construction + display ─────────────────
    support::task_with_output(
        "TemplateError variants render cleanly",
        || {
            let errs: Vec<TemplateError> = vec![
                TemplateError::InvalidSyntax("Unclosed tag".into()),
                TemplateError::RenderError("Missing key".into()),
                TemplateError::MissingVariable("name".into()),
                TemplateError::InvalidOperation(
                    "Nested templates not allowed".into(),
                ),
                TemplateError::Io(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "denied",
                )),
            ];
            errs.iter().map(|e| format!("{e}")).collect()
        },
    );

    // ── TemplateError -> EngineError conversion ─────────────────────
    support::task_with_output(
        "TemplateError lifts into EngineError via ?",
        || {
            let tpl_err =
                TemplateError::InvalidSyntax("unclosed".into());
            let engine_err: EngineError = tpl_err.into();
            vec![format!("{engine_err}")]
        },
    );

    // ── Opening a nonexistent file -> std io::Error into engine ─────
    support::task_with_output(
        "Open a bogus file, convert io::Error",
        || match File::open(temp_dir.path().join("does-not-exist")) {
            Err(e) => {
                let engine_err: EngineError = e.into();
                vec![format!("{engine_err}")]
            }
            Ok(_) => vec!["BUG: should have failed".into()],
        },
    );

    support::summary(9);
    Ok(())
}
