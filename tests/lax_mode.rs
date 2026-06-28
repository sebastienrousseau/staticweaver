// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lax-mode wire-format matrix.
//!
//! Locks the user-visible behaviour of `Engine::with_lax_undefined(true)`
//! (issue #28) so a future refactor cannot silently change what a
//! template build produces when a variable is missing.
//!
//! The contract:
//!   * **Strict** (default): an unresolved `{{x}}` returns
//!     `EngineError::Render` with a `line N, column M` suffix.
//!   * **Lax**: an unresolved `{{x}}` emits the empty string and the
//!     render continues. The filter chain attached to the unresolved
//!     tag is skipped.
//!
//! Snapshot-style assertions (`assert_eq!` on full output) deliberately
//! pin every byte, so a whitespace or escape change in the substitution
//! path is caught here, not by a downstream user.

use staticweaver::{Context, Engine};
use std::time::Duration;

fn lax() -> Engine {
    Engine::new("", Duration::from_secs(60)).with_lax_undefined(true)
}

fn strict() -> Engine {
    Engine::new("", Duration::from_secs(60))
}

#[test]
fn lax_undefined_var_emits_empty() {
    let ctx = Context::new();
    let out = lax().render_template("a={{x}}b", &ctx).unwrap();
    assert_eq!(out, "a=b");
}

#[test]
fn strict_undefined_var_errors_with_line_col() {
    let ctx = Context::new();
    let err = strict().render_template("a={{x}}b", &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Unresolved template tag: x"), "got: {msg}",);
    assert!(msg.contains("line 1"), "got: {msg}");
    assert!(msg.contains("column"), "got: {msg}");
}

#[test]
fn lax_defined_var_renders_normally() {
    // Lax must not change the substitution path when the var resolves.
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "hi".to_string());
    let out = lax().render_template("a={{x}}b", &ctx).unwrap();
    assert_eq!(out, "a=hib");
}

#[test]
fn lax_undefined_nested_path_emits_empty() {
    // `{{user.name}}` with no `user` at all must resolve to "" under lax.
    let ctx = Context::new();
    let out = lax().render_template("a={{user.name}}b", &ctx).unwrap();
    assert_eq!(out, "a=b");
}

#[test]
fn lax_multiple_undefined_vars_all_emit_empty() {
    let ctx = Context::new();
    let out =
        lax().render_template("[{{a}}|{{b}}|{{c}}]", &ctx).unwrap();
    assert_eq!(out, "[||]");
}

#[test]
fn lax_mixed_defined_and_undefined() {
    let mut ctx = Context::new();
    ctx.set("a".to_string(), "1".to_string());
    ctx.set("c".to_string(), "3".to_string());
    let out =
        lax().render_template("[{{a}}|{{b}}|{{c}}]", &ctx).unwrap();
    assert_eq!(out, "[1||3]");
}

#[test]
fn lax_undefined_tag_skips_attached_filters() {
    // Filter chain on an unresolved tag must be skipped — applying
    // e.g. `| uppercase` to the empty substitution would still be "",
    // but more importantly applying a typed filter (`| number_format`)
    // to a missing key would otherwise error and defeat the point of
    // lax mode.
    let ctx = Context::new();
    let out = lax()
        .render_template(
            r#"before [{{missing | uppercase}}] after"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "before [] after");
}

// ── Differential: strict errors AND lax succeeds on the same input ──

#[test]
fn differential_strict_errors_lax_succeeds() {
    let ctx = Context::new();
    let template = "[{{a}}|{{b}}|{{c}}]";
    assert!(strict().render_template(template, &ctx).is_err());
    assert_eq!(lax().render_template(template, &ctx).unwrap(), "[||]");
}

#[test]
fn differential_defined_input_outputs_match() {
    // When every variable resolves, strict and lax must produce the
    // same bytes — lax is strictly additive, never subtractive.
    let mut ctx = Context::new();
    ctx.set("a".to_string(), "1".to_string());
    ctx.set("b".to_string(), "2".to_string());
    let template = "[{{a}}|{{b}}]";
    let s = strict().render_template(template, &ctx).unwrap();
    let l = lax().render_template(template, &ctx).unwrap();
    assert_eq!(s, l);
    assert_eq!(s, "[1|2]");
}

#[test]
fn lax_undefined_does_not_break_escape() {
    // A defined value that contains HTML must still escape even when
    // the engine is in lax mode.
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "<b>hi</b>".to_string());
    let out = lax().render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "&lt;b&gt;hi&lt;/b&gt;");
}
