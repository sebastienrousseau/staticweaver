// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property-based robustness tests.
//!
//! The contract these tests defend: **the engine must NEVER panic
//! on arbitrary input.** Bad templates, malformed expressions, weird
//! Unicode, missing keys — all should surface as a clean
//! `EngineError`, not an unwrap or an arithmetic-overflow trap.
//!
//! Each property generates random inputs, runs them through the
//! engine, and asserts that the result is `Ok(_)` or `Err(_)` —
//! crucially, that the call returns at all.

use proptest::prelude::*;
use staticweaver::{Context, Engine};
use std::time::Duration;

fn engine() -> Engine {
    Engine::new("", Duration::from_secs(60))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        // Keep test runs fast under `cargo test`; raise locally for
        // longer fuzz sessions.
        ..ProptestConfig::default()
    })]

    /// Arbitrary text (non-empty) must never panic the renderer.
    /// Outcome is either Ok with the text rendered, or a clean error
    /// (no template content); both are acceptable.
    #[test]
    fn render_template_never_panics_on_arbitrary_text(
        s in "\\PC{1,200}",
    ) {
        let e = engine();
        let ctx = Context::new();
        let _ = e.render_template(&s, &ctx);
    }

    /// Random templates that LOOK like they have tags
    /// (interleaving `{{`, `}}`, identifier-ish bytes) must not
    /// panic. Many will be malformed; the engine should err
    /// cleanly.
    #[test]
    fn render_template_never_panics_on_taglike_text(
        s in "[a-z{}#/!@_.|]{1,300}",
    ) {
        let e = engine();
        let ctx = Context::new();
        let _ = e.render_template(&s, &ctx);
    }

    /// Random `#if` expressions inside a wrapper template should
    /// never panic — the recursive-descent expression parser is the
    /// most algorithmically complex piece and the most likely to
    /// hide an unwrap.
    #[test]
    fn if_expressions_never_panic(
        expr in "[a-z0-9 +\\-*/<>=!~()'\"]{1,80}",
    ) {
        let template = format!("{{{{#if {expr}}}}}y{{{{else}}}}n{{{{/if}}}}");
        let e = engine();
        let ctx = Context::new();
        let _ = e.render_template(&template, &ctx);
    }

    /// Random key names and values via Context — substitution
    /// should never panic regardless of bytes set into either.
    #[test]
    fn substitution_never_panics_on_arbitrary_context_value(
        key in "[a-z][a-z0-9_]{0,15}",
        value in "\\PC{0,200}",
    ) {
        let mut ctx = Context::new();
        ctx.set(key.clone(), value);
        let template = format!("hi {{{{ {key} }}}}");
        let e = engine();
        let _ = e.render_template(&template, &ctx);
    }

    /// Whitespace-control delimiters in arbitrary positions must
    /// not panic. Many won't form a valid tag; that's fine — they
    /// should error or render literally.
    #[test]
    fn whitespace_control_never_panics(
        before in "[ \\t\\n]{0,5}",
        body in "[a-z_]{1,8}",
        after in "[ \\t\\n]{0,5}",
    ) {
        let template =
            format!("{before}{{{{- {body} -}}}}{after}");
        let e = engine();
        let mut ctx = Context::new();
        ctx.set(body, "v".to_string());
        let _ = e.render_template(&template, &ctx);
    }

    /// Random math expressions (including divisions, negatives,
    /// big integers near i64 bounds) must error gracefully on
    /// overflow/divide-by-zero rather than panicking.
    #[test]
    fn math_expressions_never_panic(
        a in any::<i64>(),
        b in any::<i64>(),
        op in proptest::sample::select(vec!["+", "-", "*", "/"]),
    ) {
        let template = format!(
            "{{{{#if {a} {op} {b} == 0}}}}y{{{{else}}}}n{{{{/if}}}}"
        );
        let e = engine();
        let ctx = Context::new();
        let _ = e.render_template(&template, &ctx);
    }
}
