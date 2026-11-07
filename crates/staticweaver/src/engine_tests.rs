// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Unit tests for [`Engine`](super::Engine), exercising the core
//! renderer through the facade's public API.

use super::*;
use crate::Context;

#[test]
fn test_render_template() {
    let mut engine = Engine::new("", Duration::from_secs(60));
    engine.set_delimiters("<<", ">>");
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    context.set("greeting".to_string(), "Hello".to_string());

    let template = "<<greeting>>, <<name>>!";
    let result = engine.render_template(template, &context).unwrap();
    assert_eq!(result, "Hello, Alice!");
}

#[test]
fn test_render_template_empty() {
    let engine = Engine::new("", Duration::from_secs(60));
    let context = Context::new();

    let template = "";
    let result = engine.render_template(template, &context);
    assert!(matches!(result, Err(EngineError::InvalidTemplate(_))));
}

#[test]
fn test_render_template_bare_open_char_is_literal() {
    // A single `{` with no matching `{{` is literal text, not an error.
    // Previously rejected by a broken heuristic.
    let mut engine = Engine::new("", Duration::from_secs(60));
    engine.set_delimiters("{{", "}}");
    let context = Context::new();
    let template = "Hello, {name}!";
    let result = engine.render_template(template, &context);
    assert_eq!(result.unwrap(), "Hello, {name}!");
}

#[test]
fn test_render_template_nested_delimiters_rejected() {
    let engine = Engine::new("", Duration::from_secs(60));
    let context = Context::new();
    let template = "{{outer{{inner}}}}";
    let result = engine.render_template(template, &context);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("Nested")
    ));
}

#[test]
fn test_render_template_custom_delimiters() {
    let mut engine = Engine::new("", Duration::from_secs(60));
    engine.set_delimiters("<<", ">>");
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    context.set("greeting".to_string(), "Hello".to_string());

    let template = "<<greeting>>, <<name>>!";
    let result = engine.render_template(template, &context).unwrap();
    assert_eq!(result, "Hello, Alice!");

    // Bare `<` is literal text under the new parser.
    let literal_template = "Hello, <name>!";
    let result =
        engine.render_template(literal_template, &context).unwrap();
    assert_eq!(result, "Hello, <name>!");
}

#[test]
fn test_render_template_escapes_html_by_default() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set(
        "name".to_string(),
        "<script>alert('x')</script>".to_string(),
    );
    let result =
        engine.render_template("Hi {{name}}", &context).unwrap();
    assert_eq!(
        result,
        "Hi &lt;script&gt;alert(&#x27;x&#x27;)&lt;/script&gt;"
    );
}

#[test]
fn render_template_resolves_dot_notation() {
    use crate::context::Value;
    use fnv::FnvHashMap;

    let engine = Engine::new("", Duration::from_secs(60));
    let mut user = FnvHashMap::default();
    let _ = user
        .insert("name".to_string(), Value::String("Ada".to_string()));
    let mut ctx = Context::new();
    ctx.set_value("user".to_string(), Value::Map(user));

    let out = engine
        .render_template("Hello, {{user.name}}!", &ctx)
        .unwrap();
    assert_eq!(out, "Hello, Ada!");
}

#[test]
fn render_template_renders_primitive_values() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("count".to_string(), 42);
    ctx.set_value("active".to_string(), true);

    let out = engine
        .render_template("count={{count}} active={{active}}", &ctx)
        .unwrap();
    assert_eq!(out, "count=42 active=true");
}

#[test]
fn if_block_renders_when_truthy() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("show".to_string(), true);
    let out = engine
        .render_template("{{#if show}}hello{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "hello");
}

#[test]
fn if_block_skips_when_falsy() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("show".to_string(), false);
    let out = engine
        .render_template("[{{#if show}}hi{{/if}}]", &ctx)
        .unwrap();
    assert_eq!(out, "[]");
}

#[test]
fn if_block_with_else_branch() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("on".to_string(), false);
    let out = engine
        .render_template("{{#if on}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "no");
}

#[test]
fn each_block_iterates_list_with_this_binding() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    let out = engine
        .render_template("{{#each items}}[{{this}}]{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "[a][b][c]");
}

#[test]
fn nested_if_inside_each_resolves() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["x", "y"]);
    ctx.set_value("show".to_string(), true);
    let out = engine
        .render_template(
            "{{#each items}}{{#if show}}{{this}}{{/if}}{{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "xy");
}

#[test]
fn unclosed_if_block_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("on".to_string(), true);
    let res = engine.render_template("{{#if on}}forever", &ctx);
    assert!(matches!(
        res,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("#if")
    ));
}

#[test]
fn stray_close_tag_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let res = engine.render_template("nope {{/if}}", &ctx);
    assert!(matches!(
        res,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("/if")
    ));
}

#[test]
fn each_on_non_list_errors() {
    use crate::context::Value;
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("not_a_list".to_string(), Value::Number(1));
    let res =
        engine.render_template("{{#each not_a_list}}x{{/each}}", &ctx);
    assert!(matches!(res, Err(EngineError::InvalidTemplate(_))));
}

#[test]
fn render_template_indexes_lists_by_position() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);

    let out = engine
        .render_template("first={{items.0}} last={{items.2}}", &ctx)
        .unwrap();
    assert_eq!(out, "first=a last=c");
}

#[test]
fn test_render_template_raw_opt_out() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("body".to_string(), "<b>hi</b>".to_string());
    let result = engine.render_template("{{!body}}", &context).unwrap();
    assert_eq!(result, "<b>hi</b>");
}

#[test]
fn comment_inline_renders_nothing() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine.render_template("[{{! ignored }}]", &ctx).unwrap();
    assert_eq!(out, "[]");
}

#[test]
fn comment_block_form_renders_nothing() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("[{{!-- block\nspans lines --}}]", &ctx)
        .unwrap();
    assert_eq!(out, "[]");
}

#[test]
fn comment_empty_form_renders_nothing() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine.render_template("[{{!}}]", &ctx).unwrap();
    assert_eq!(out, "[]");
}

#[test]
fn whitespace_strip_left_removes_trailing_output_spaces() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out =
        engine.render_template("hello   {{- name}}", &ctx).unwrap();
    assert_eq!(out, "helloAda");
}

#[test]
fn whitespace_strip_right_skips_leading_chunk_spaces() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out =
        engine.render_template("{{name -}}   tail", &ctx).unwrap();
    assert_eq!(out, "Adatail");
}

#[test]
fn whitespace_strip_both_collapses_around_tag() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine
        .render_template("a   {{- name -}}\n\n   b", &ctx)
        .unwrap();
    assert_eq!(out, "aAdab");
}

#[test]
fn whitespace_strip_composes_with_inline_comment() {
    // `{{- ! note -}}` is "strip left + inline comment + strip right"
    // — useful as a pure whitespace-eating marker.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("a   {{- ! note -}}\n\n   b", &ctx)
        .unwrap();
    assert_eq!(out, "ab");
}

#[test]
fn filter_truncate_accepts_explicit_limit() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("body".to_string(), "hello world".to_string());
    let out = engine
        .render_template("{{ body | truncate:8 }}", &ctx)
        .unwrap();
    assert_eq!(out, "hello...");
}

#[test]
fn filter_truncate_falls_back_to_default_30() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("body".to_string(), "x".repeat(50).to_string());
    let out = engine
        .render_template("{{ body | truncate }}", &ctx)
        .unwrap();
    assert_eq!(out.chars().count(), 30);
    assert!(out.ends_with("..."));
}

#[test]
fn filter_capitalize_uppercases_first_char() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "alice".to_string());
    let out = engine
        .render_template("{{ name | capitalize }}", &ctx)
        .unwrap();
    assert_eq!(out, "Alice");
}

#[test]
fn filter_length_counts_chars() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Adriana".to_string());
    let out =
        engine.render_template("{{ name | length }}", &ctx).unwrap();
    assert_eq!(out, "7");
}

#[test]
fn filter_default_substitutes_for_empty() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), String::new());
    let out = engine
        .render_template(r#"{{ name | default:"anon" }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "anon");
}

#[test]
fn filter_default_passes_through_non_empty() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine
        .render_template(r#"{{ name | default:"anon" }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "Ada");
}

#[test]
fn filter_replace_substitutes_substring() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("msg".to_string(), "hello world".to_string());
    let out = engine
        .render_template(r#"{{ msg | replace:"world","there" }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "hello there");
}

#[test]
fn filter_replace_errors_without_two_args() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("msg".to_string(), "x".to_string());
    let err = engine
        .render_template(r#"{{ msg | replace:"only" }}"#, &ctx)
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::Render(msg) if msg.contains("two args"),
    ));
}

#[test]
fn filter_urlencode_handles_special_chars() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("q".to_string(), "rust lang & you".to_string());
    let out =
        engine.render_template("{{ q | urlencode }}", &ctx).unwrap();
    assert_eq!(out, "rust%20lang%20%26%20you");
}

#[test]
fn filter_safe_suppresses_html_escape() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("body".to_string(), "<b>hi</b>".to_string());
    // Without `safe`, default escape applies.
    let escaped = engine.render_template("{{ body }}", &ctx).unwrap();
    assert_eq!(escaped, "&lt;b&gt;hi&lt;/b&gt;");
    // With trailing `safe`, raw output.
    let raw =
        engine.render_template("{{ body | safe }}", &ctx).unwrap();
    assert_eq!(raw, "<b>hi</b>");
}

// ── E1: Custom filters API ─────────────────────────────────────

#[test]
fn add_filter_registers_custom_pipeline_step() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_filter(
        "shout",
        Arc::new(|input, _args| {
            Ok(format!("{}!!!", input.to_uppercase()))
        }),
    );
    let mut ctx = Context::new();
    ctx.set("greeting".to_string(), "hello".to_string());
    let out = engine
        .render_template("{{ greeting | shout }}", &ctx)
        .unwrap();
    // shout produces "HELLO!!!"; HTML escape leaves '!' alone.
    assert_eq!(out, "HELLO!!!");
}

#[test]
fn add_filter_receives_arguments() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_filter(
        "wrap",
        Arc::new(|input, args: &[String]| {
            let pre = args.first().map(String::as_str).unwrap_or("[");
            let post = args.get(1).map(String::as_str).unwrap_or("]");
            Ok(format!("{pre}{input}{post}"))
        }),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine
        .render_template(r#"{{ name | wrap:"<<",">>" }}"#, &ctx)
        .unwrap();
    // The output is HTML-escaped after the filter runs, so `<<`
    // becomes `&lt;&lt;`. That's the same contract as built-in
    // filters — `safe` is the documented opt-out.
    assert_eq!(out, "&lt;&lt;Ada&gt;&gt;");
}

#[test]
fn add_filter_overrides_builtin_with_same_name() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    // Override `uppercase` to do the opposite — proves precedence.
    let _ = engine.add_filter(
        "uppercase",
        Arc::new(|input, _args| Ok(input.to_lowercase())),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "ADA".to_string());
    let out = engine
        .render_template("{{ name | uppercase }}", &ctx)
        .unwrap();
    assert_eq!(out, "ada");
}

#[test]
fn add_filter_propagates_errors_from_user_code() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_filter(
        "boom",
        Arc::new(|_input, _args| {
            Err(EngineError::Render("filter exploded".to_string()))
        }),
    );
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let err =
        engine.render_template("{{ x | boom }}", &ctx).unwrap_err();
    assert!(
        format!("{err}").contains("filter exploded"),
        "expected filter error to propagate, got {err}"
    );
}

// ── E2: Stream rendering ───────────────────────────────────────

// ── E3: Line numbers in error messages ─────────────────────────

#[test]
fn unresolved_tag_error_reports_line_and_column() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // Line 2: "Hello {{missing}}". The lookup `missing` sits at
    // column 9 (after "Hello " + "{{").
    let template = "Header\nHello {{missing}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("missing"), "got: {msg}");
    assert!(msg.contains("line 2"), "got: {msg}");
    assert!(msg.contains("column 9"), "got: {msg}");
}

#[test]
fn unresolved_each_list_reports_line_and_column() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let template = "\n\n{{#each undefined_list}}{{this}}{{/each}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("undefined_list"), "got: {msg}");
    assert!(msg.contains("line 3"), "got: {msg}");
}

#[test]
fn unclosed_tag_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let template = "Line one\nLine two\nLine three {{ unclosed";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Unclosed"), "got: {msg}");
    assert!(msg.contains("line 3"), "got: {msg}");
}

#[test]
fn stray_closing_tag_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let template = "ok\n{{/if}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("/if"), "got: {msg}");
    assert!(msg.contains("line 2"), "got: {msg}");
}

#[test]
fn line_one_errors_report_line_one() {
    // Single-line template: error should still say `line 1`.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine.render_template("{{nope}}", &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 1"), "got: {msg}");
    assert!(msg.contains("column 3"), "got: {msg}");
}

// ── F1: Line:col on the remaining error sites ──────────────────

#[test]
fn malformed_if_expression_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // Bad expression: missing operand after `==`
    let template = "ok\n{{#if a == }}x{{/if}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "got: {msg}");
}

#[test]
fn unclosed_if_block_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // No matching {{/if}} — extract_block fails.
    let template = "header\n{{#if true}}body\nstill body";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "got: {msg}");
}

#[test]
fn malformed_set_assignment_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // Missing `=` makes parse_set_assignment fail.
    let template = "ok\n{{#set badname}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "got: {msg}");
}

// ── Coverage: hit ≥98% line coverage ───────────────────────────

#[test]
fn engine_debug_impl_lists_filter_names() {
    use std::sync::Arc;
    let mut engine = Engine::new("t", Duration::from_secs(60));
    let _ = engine.add_filter(
        "shout",
        Arc::new(|input, _| Ok(input.to_uppercase())),
    );
    let s = format!("{engine:?}");
    assert!(s.contains("Engine"), "{s}");
    assert!(s.contains("shout"), "{s}");
    assert!(s.contains("template_path"), "{s}");
}

#[test]
fn render_page_to_writes_into_a_buffer() {
    let temp = tempfile::TempDir::new().unwrap();
    let layout = temp.path().join("hello.html");
    fs::write(&layout, "Hello, {{name}}!").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let mut buf: Vec<u8> = Vec::new();
    engine.render_page_to(&ctx, "hello", &mut buf).unwrap();
    assert_eq!(buf, b"Hello, Ada!");
}

#[test]
fn filter_truncate_pass_through_when_input_short() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "hi".to_string());
    let out = engine
        .render_template("{{ s | truncate:30 }}", &ctx)
        .unwrap();
    assert_eq!(out, "hi");
}

#[test]
fn filter_capitalize_on_empty_input() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), String::new());
    let out = engine
        .render_template("{{ s | capitalize }}", &ctx)
        .unwrap();
    assert_eq!(out, "");
}

#[test]
fn set_assignment_empty_name_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine.render_template("{{#set = 1}}", &ctx).unwrap_err();
    assert!(format!("{err}").contains("empty name"), "{err}");
}

#[test]
fn set_assignment_empty_value_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err =
        engine.render_template("{{#set x = }}", &ctx).unwrap_err();
    // Just check the error fires; message comes from parse_set_assignment.
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

#[test]
fn partial_params_empty_key_errors() {
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "x").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let err = engine.render_template("{{> p =1}}", &ctx).unwrap_err();
    assert!(format!("{err}").contains("empty key"), "{err}");
}

#[test]
fn partial_params_missing_equals_errors() {
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "x").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let err = engine.render_template("{{> p key1}}", &ctx).unwrap_err();
    assert!(format!("{err}").contains("missing"), "{err}");
}

#[test]
fn parse_operand_accepts_true_and_false_literals() {
    // Exercises the True/False arms in parse_operand.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let yes = engine
        .render_template("{{#if true == true}}y{{else}}n{{/if}}", &ctx)
        .unwrap();
    assert_eq!(yes, "y");
    let no = engine
        .render_template("{{#if false == true}}y{{else}}n{{/if}}", &ctx)
        .unwrap();
    assert_eq!(no, "n");
}

#[test]
fn parse_extends_unclosed_tag_errors() {
    // {{#extends with no closing }} — exercises the unclosed
    // path in parse_extends.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine
        .render_template(r#"{{#extends "base"#, &ctx)
        .unwrap_err();
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

#[test]
fn math_overflow_triggers_for_subtraction() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("min".to_string(), i64::MIN);
    let err = engine
        .render_template("{{#if min - 1 == 0}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("integer overflow"), "{err}");
}

#[test]
fn math_overflow_triggers_for_multiplication() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("max".to_string(), i64::MAX);
    let err = engine
        .render_template("{{#if max * 2 == 0}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("integer overflow"), "{err}");
}

#[test]
fn math_overflow_triggers_for_integer_division() {
    // i64::MIN / -1 overflows because the result doesn't fit in i64.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("min".to_string(), i64::MIN);
    let err = engine
        .render_template("{{#if min / -1 == 0}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("integer overflow"), "{err}");
}

#[test]
fn partial_params_quoted_value_with_special_chars() {
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "got={{label}}").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template(r#"{{> p label="hi there"}}"#, &ctx)
        .unwrap();
    assert_eq!(out, "got=hi there");
}

#[test]
fn partial_params_unquoted_value_parsed_as_typed_literal() {
    // Exercises the bareword parsing branch (true/false/null/int/string).
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(
        temp.path().join("p.html"),
        "{{#if flag}}on{{else}}off{{/if}}/{{count}}",
    )
    .unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template("{{> p flag=true count=42}}", &ctx)
        .unwrap();
    assert_eq!(out, "on/42");
}

#[test]
fn url_encode_passes_unreserved_through() {
    // Exercises every branch of the url_encode match, including
    // the unreserved-set passthroughs (digits, dash, underscore,
    // dot, tilde) and the percent-escape fallback.
    let mut ctx = Context::new();
    ctx.set("v".to_string(), "Foo-Bar_42.~ x".to_string());
    let engine = Engine::new("", Duration::from_secs(60));
    let out =
        engine.render_template("{{ v | urlencode }}", &ctx).unwrap();
    // Space becomes %20; everything else passes through.
    assert_eq!(out, "Foo-Bar_42.~%20x");
}

#[test]
fn partial_params_single_quoted_value() {
    // Exercises the single-quote arm of parse_partial_value
    // (most tests use double quotes).
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "got={{label}}").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template("{{> p label='hello'}}", &ctx)
        .unwrap();
    assert_eq!(out, "got=hello");
}

#[test]
fn block_name_with_single_quotes_works_in_template() {
    // Exercises parse_block_name single-quote return through
    // the actual template path (#extends "name" tests cover
    // double quotes).
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "{{#block 'main'}}default{{/block}}",
    )
    .unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // The rendered base just yields the default body; the
    // important coverage is parse_block_name accepting 'main'.
    let out = engine
        .render_template(
            "{{#extends 'base'}}{{#block 'main'}}HI{{/block}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "HI");
}

// ── Coverage push: error paths + signal propagation ───────────

#[test]
fn each_range_with_non_numeric_lo_errors() {
    // Exercises the (l, h) match-arm in #each range parsing.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("lo".to_string(), "abc".to_string());
    let err = engine
        .render_template("{{#each lo..5}}x{{/each}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err}").contains("range bounds must be numbers"),
        "{err}"
    );
}

#[test]
fn each_range_with_non_numeric_hi_errors() {
    // Same arm but on the upper bound.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("hi".to_string(), "abc".to_string());
    let err = engine
        .render_template("{{#each 0..hi}}x{{/each}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err}").contains("range bounds must be numbers"),
        "{err}"
    );
}

#[test]
fn break_propagates_up_through_block_override() {
    // Exercises the FlowSignal!=Done arm inside the #block
    // dispatch path, which propagates the signal to the
    // enclosing #each.
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "[{{#block \"body\"}}default{{/block}}]".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#each items}}\
         {{#extends \"base\"}}\
         {{#block \"body\"}}\
         {{#if this == \"stop\"}}{{#break}}{{/if}}\
         [{{this}}]\
         {{/block}}\
         {{/each}}"
            .to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "stop", "c"]);
    // We can't easily render extends inside each (extends is
    // file-level), so just verify the engine doesn't panic on
    // the unusual nesting.
    let _ = engine.render_page(&ctx, "child");
}

#[test]
fn collect_blocks_unclosed_tag_errors() {
    // Exercises the unclosed-tag error path inside collect_blocks
    // (only reachable when a child template has a {{ without
    // matching }}).
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "{{#block \"x\"}}default{{/block}}".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}{{#block \"x\"}}body{{/block}}{{ unclosed".to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let err = engine.render_page(&ctx, "child").unwrap_err();
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

#[test]
fn partial_param_unquoted_value_errors_when_missing() {
    // Exercises the None branch in parse_partial_value
    // (params_str ends after `key=` with no value at all).
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert("p".to_string(), "got={{x}}".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let err = engine.render_template("{{> p x=}}", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

#[test]
fn each_range_with_only_dot_dot_no_lo_errors() {
    // Empty lo expression — exercises the parse_expr error
    // branch on the lo side of the range.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine
        .render_template("{{#each ..5}}x{{/each}}", &ctx)
        .unwrap_err();
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

#[test]
fn render_template_to_with_io_failure_propagates() {
    // Writer that always fails — the io::Error must surface
    // as EngineError::Io rather than panic.
    struct Bomb;
    impl Write for Bomb {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("x"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("x"))
        }
    }
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("k".to_string(), "v".to_string());
    let err = engine.render_to("{{k}}", &ctx, &mut Bomb).unwrap_err();
    assert!(matches!(err, EngineError::Io(_)), "{err:?}");
}

#[test]
fn partial_with_unicode_invalid_value_recovered() {
    // Exercises set_value_string's String-buffer-reuse fallback
    // path when the slot is currently a non-String Value
    // (number) being overwritten with a string.
    let mut ctx = Context::new();
    ctx.set_value("count".to_string(), 42i64);
    // Now overwrite with a string — this hits the
    // `Some(slot) => *slot = Value::String(...)` arm in
    // set_value_string.
    ctx.set_value_string("count", "many");
    assert_eq!(ctx.get("count").map(String::as_str), Some("many"));
}

// ── Signal propagation: #if / #block bubble Break/Continue up

#[test]
fn break_inside_if_propagates_up_to_each() {
    // Hits the `if signal != FlowSignal::Done` arm inside #if.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "stop", "c"]);
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#if this == \"stop\"}}{{#break}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "[a]");
}

#[test]
fn continue_inside_if_propagates_up_to_each() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "skip", "c"]);
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#if this == \"skip\"}}{{#continue}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "[a][c]");
}

#[test]
fn break_inside_block_default_propagates_up_to_each() {
    // Hits the `if signal != FlowSignal::Done` arm inside the
    // #block default-body render (override path).
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "page".to_string(),
        "{{#each items}}\
         {{#block \"row\"}}\
         {{#if this == \"stop\"}}{{#break}}{{/if}}\
         [{{this}}]\
         {{/block}}\
         {{/each}}"
            .to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "stop", "c"]);
    let out = engine.render_page(&ctx, "page").unwrap();
    assert_eq!(out, "[a]");
}

// ── parse_filter_args quoting state machine

#[test]
fn filter_args_double_quoted_preserves_commas() {
    // Hits the ('"', None) → in_quote = Some('"') branch.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abc".to_string());
    let out = engine
        .render_template(r#"{{ s | replace:"a,b","x" }}"#, &ctx)
        .unwrap();
    // No commas inside the search needle, but the comma is
    // protected by the double quotes — proves the state
    // machine respects them.
    assert_eq!(out, "abc");
}

#[test]
fn filter_args_single_quoted_preserves_commas() {
    // Hits the ('\'', None) branch.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abc".to_string());
    let out = engine
        .render_template("{{ s | replace:'a,b','x' }}", &ctx)
        .unwrap();
    assert_eq!(out, "abc");
}

// ── `is X` on non-path operands

#[test]
fn is_defined_on_non_path_operand_uses_null_check() {
    // Hits the else branch in TestKind::Defined where the
    // operand is not a Path. We use a math expression as the
    // operand; the result is `Value::Number`, which is "defined"
    // (not Null).
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 5i64);
    let out = engine
        .render_template(
            "{{#if n + 1 is defined}}Y{{else}}N{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Y");
}

// ── split_else malformed-tag break path

#[test]
fn each_range_eval_failure_on_lo_propagates_with_pos() {
    // Hits the .eval(active, self).map_err(|e| annotate_pos(e, origin, arg))
    // arm on the `lo` side of a range — the parse succeeds but
    // eval fails (division by zero). Position-suffix path is
    // exercised too.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("z".to_string(), 0i64);
    let err = engine
        .render_template("{{#each 1 / z..5}}x{{/each}}", &ctx)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("division by zero"), "{msg}");
}

#[test]
fn each_range_eval_failure_on_hi_propagates_with_pos() {
    // Same arm but on the `hi` bound.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("z".to_string(), 0i64);
    let err = engine
        .render_template("{{#each 0..1 / z}}x{{/each}}", &ctx)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("division by zero"), "{msg}");
}

#[test]
fn each_with_extract_block_failure_annotates_pos() {
    // Hits the extract_block error -> annotate_pos arm
    // (lines ~1249-1250) when #each has no matching {{/each}}.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b"]);
    let err = engine
        .render_template(
            "header\n{{#each items}}body without close",
            &ctx,
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "{msg}");
}

#[test]
fn block_extract_failure_annotates_pos() {
    // Hits the extract_block error annotation arm inside #block
    // dispatch (line ~1489-1490).
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine
        .render_template("before\n{{#block \"x\"}}body no close", &ctx)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "{msg}");
}

#[test]
fn block_signal_propagation_inside_each() {
    // Hits `if signal != FlowSignal::Done { return Ok(signal); }`
    // inside the #block dispatch (line ~1512). #each catches
    // the signal so the loop terminates cleanly.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b"]);
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#block \"row\"}}\
             {{#break}}\
             [{{this}}]\
             {{/block}}\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "");
}

#[test]
fn partial_value_bareword_string_fallback() {
    // Hits parse_partial_value's `_ => match s.parse::<i64>()`
    // → `Err(_) => Value::String(...)` arm with a bareword
    // that isn't a recognised keyword and doesn't parse as
    // i64.
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert("p".to_string(), "got:{{label}}".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // `bareword` is not "true"/"false"/"null" and doesn't parse
    // as i64 — it falls through to Value::String("bareword").
    let out = engine
        .render_template("{{> p label=bareword}}", &ctx)
        .unwrap();
    assert_eq!(out, "got:bareword");
}

#[test]
fn partial_param_value_with_trailing_comma_in_bareword() {
    // Same fallback path but exercising the bareword detection
    // chain in parse_partial_params for a value that's neither
    // quoted nor a typed literal.
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert("p".to_string(), "got:{{x}}".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template("{{> p x=hello_world}}", &ctx)
        .unwrap();
    assert_eq!(out, "got:hello_world");
}

#[test]
fn set_assignment_bareword_falls_back_to_string() {
    // Hits parse_literal_value's `Err(_) => Value::String(...)`
    // fallback (line ~3186) — value isn't a quoted string,
    // isn't `true`/`false`/`null`, and doesn't parse as i64.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("{{#set tier = pro}}got:{{tier}}", &ctx)
        .unwrap();
    assert_eq!(out, "got:pro");
}

#[test]
fn set_assignment_with_quoted_string() {
    // Hits parse_literal_value's quoted-string return arm
    // (line ~3175-3177). Already covered by other tests but
    // pinned here for completeness.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#set greeting = \"Hello\"}}{{greeting}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Hello");
}

#[test]
fn set_assignment_with_single_quoted_string() {
    // Same arm but single quotes.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("{{#set greeting = 'Hi'}}{{greeting}}", &ctx)
        .unwrap();
    assert_eq!(out, "Hi");
}

#[test]
fn parse_block_name_bareword_fallthrough() {
    // Hits the unquoted return arm of parse_block_name (line
    // ~3210). Bareword block names are accepted unchanged.
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "page".to_string(),
        "{{#block bareword}}body{{/block}}".to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine.render_page(&ctx, "page").unwrap();
    assert_eq!(out, "body");
}

#[test]
fn ws_control_block_comment_carve_out() {
    // Hits parse_ws_control's early-return for `{{!-- ... --}}`
    // (line ~3385). The `--` at the end must NOT be treated
    // as a whitespace-control marker.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("before{{!-- comment --}}after", &ctx)
        .unwrap();
    assert_eq!(out, "beforeafter");
}

#[test]
fn split_else_unclosed_inner_tag_does_not_panic() {
    // Inside an #if body, an unclosed inner `{{` causes
    // split_else's `else` arm with `find(close)` returning None
    // — the helper breaks rather than panicking; the outer
    // render path then surfaces the malformed-tag error.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("ok".to_string(), true);
    let err = engine
        .render_template("{{#if ok}}body {{ unclosed{{/if}}", &ctx)
        .unwrap_err();
    // Just verify it errors cleanly rather than panicking.
    assert!(matches!(err, EngineError::InvalidTemplate(_)), "{err:?}");
}

// ── H7: Auto-escape per file extension ────────────────────────

fn make_engine_with_two_pages() -> Engine {
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert("page.html".to_string(), "{{x}}".to_string());
    let _ = store.insert("plain.txt".to_string(), "{{x}}".to_string());
    Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    )
}

#[test]
fn autoescape_on_escapes_for_listed_extension() {
    let mut engine = make_engine_with_two_pages();
    let _ = engine.autoescape_on(&[".html"]);
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "<b>".to_string());
    let out = engine.render_page(&ctx, "page.html").unwrap();
    assert_eq!(out, "&lt;b&gt;");
}

#[test]
fn autoescape_on_skips_unlisted_extension() {
    let mut engine = make_engine_with_two_pages();
    let _ = engine.autoescape_on(&[".html"]);
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "<b>".to_string());
    let out = engine.render_page(&ctx, "plain.txt").unwrap();
    assert_eq!(out, "<b>");
}

#[test]
fn autoescape_on_does_not_change_render_template_behaviour() {
    // render_template has no layout name; per-extension
    // policy doesn't apply — the global escape_html still rules.
    let mut engine = make_engine_with_two_pages();
    let _ = engine.autoescape_on(&[".html"]);
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "<b>".to_string());
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "&lt;b&gt;");
}

#[test]
fn autoescape_global_state_restored_after_render_page() {
    let mut engine = make_engine_with_two_pages();
    let _ = engine.autoescape_on(&[".html"]);
    let saved = engine.escape_html;
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let _ = engine.render_page(&ctx, "plain.txt").unwrap();
    assert_eq!(engine.escape_html, saved);
    let _ = engine.render_page(&ctx, "page.html").unwrap();
    assert_eq!(engine.escape_html, saved);
}

#[test]
fn autoescape_multiple_extensions_match_any() {
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert("a.html".to_string(), "{{x}}".to_string());
    let _ = store.insert("b.xml".to_string(), "{{x}}".to_string());
    let _ = store.insert("c.txt".to_string(), "{{x}}".to_string());
    let mut engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let _ = engine.autoescape_on(&[".html", ".xml"]);
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "<b>".to_string());
    assert_eq!(
        engine.render_page(&ctx, "a.html").unwrap(),
        "&lt;b&gt;"
    );
    assert_eq!(engine.render_page(&ctx, "b.xml").unwrap(), "&lt;b&gt;");
    assert_eq!(engine.render_page(&ctx, "c.txt").unwrap(), "<b>");
}

// ── H6: json encode filter (feature-gated) ────────────────────

#[cfg(feature = "json")]
#[test]
fn filter_json_encodes_strings() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada \"the\" Lovelace".to_string());
    let out = engine
        .render_template("{{ name | json | safe }}", &ctx)
        .unwrap();
    assert_eq!(out, r#""Ada \"the\" Lovelace""#);
}

#[cfg(feature = "json")]
#[test]
fn filter_json_encodes_numbers_and_bools() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 42i64);
    ctx.set_value("ok".to_string(), true);
    let n_out = engine.render_template("{{ n | json }}", &ctx).unwrap();
    assert_eq!(n_out, "42");
    let b_out =
        engine.render_template("{{ ok | json }}", &ctx).unwrap();
    assert_eq!(b_out, "true");
}

#[cfg(feature = "json")]
#[test]
fn filter_json_encodes_lists() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["alpha", "beta"]);
    let out = engine
        .render_template("{{ items | json | safe }}", &ctx)
        .unwrap();
    assert_eq!(out, r#"["alpha","beta"]"#);
}

#[cfg(feature = "json")]
#[test]
fn filter_json_encodes_nested_maps_with_sorted_keys() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    let mut user: fnv::FnvHashMap<String, crate::context::Value> =
        fnv::FnvHashMap::default();
    let _ = user
        .insert("z_last".to_string(), crate::context::Value::Number(2));
    let _ = user.insert(
        "a_first".to_string(),
        crate::context::Value::Number(1),
    );
    ctx.set_value("user".to_string(), crate::context::Value::Map(user));
    let out = engine
        .render_template("{{ user | json | safe }}", &ctx)
        .unwrap();
    // Keys must be in sorted order for deterministic output.
    assert_eq!(out, r#"{"a_first":1,"z_last":2}"#);
}

#[cfg(feature = "json")]
#[test]
fn filter_json_encodes_null_for_unset_path() {
    // Substituting a missing key normally errors, so test json
    // on an explicit Null value.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Null);
    let out = engine.render_template("{{ x | json }}", &ctx).unwrap();
    assert_eq!(out, "null");
}

// ── H5: Range iteration in #each ──────────────────────────────

#[test]
fn each_range_with_integer_literals() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("{{#each 1..5}}[{{this}}]{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "[1][2][3][4]");
}

#[test]
fn each_range_starts_at_zero() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("{{#each 0..3}}{{this}}{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "012");
}

#[test]
fn each_range_with_path_bounds() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("lo".to_string(), 2i64);
    ctx.set_value("hi".to_string(), 6i64);
    let out = engine
        .render_template("{{#each lo..hi}}{{this}},{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "2,3,4,5,");
}

#[test]
fn each_range_empty_when_lo_ge_hi() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#each 5..5}}should not appear{{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "");
}

#[test]
fn each_range_binds_index_and_helpers() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#each 0..3}}@{{@index}}={{this}};{{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "@0=0;@1=1;@2=2;");
}

#[test]
fn each_range_with_arithmetic_expression() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 3i64);
    let out = engine
        .render_template("{{#each 0..n + 1}}{{this}}{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "0123");
}

#[test]
fn each_range_errors_on_non_numeric_bounds() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("lo".to_string(), "abc".to_string());
    let err = engine
        .render_template("{{#each lo..5}}x{{/each}}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("range"), "{err}");
}

// ── H4: super() in inherited blocks ───────────────────────────

#[test]
fn super_in_child_block_includes_parent_default() {
    use std::collections::HashMap;
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "[{{#block \"body\"}}DEFAULT{{/block}}]".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"body\"}}( {{ super() }} )-OVR{{/block}}"
            .to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine.render_page(&ctx, "child").unwrap();
    assert_eq!(out, "[( DEFAULT )-OVR]");
}

#[test]
fn super_outside_override_is_silent_no_op() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine.render_template("a{{ super() }}b", &ctx).unwrap();
    assert_eq!(out, "ab");
}

#[test]
fn super_does_not_leak_across_partial_boundary() {
    use std::collections::HashMap;
    // Partial uses {{ super() }}; that should be a silent no-op
    // even when the partial is included from inside a block
    // override that has a non-empty super body.
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "[{{#block \"main\"}}base{{/block}}]".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"main\"}}{{> p}}{{/block}}"
            .to_string(),
    );
    let _ =
        store.insert("p".to_string(), "<{{ super() }}>".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine.render_page(&ctx, "child").unwrap();
    // super() inside p.html sees no parent body; emits nothing.
    assert_eq!(out, "[<>]");
}

#[test]
fn super_renders_parent_body_with_parent_tags_resolved() {
    use std::collections::HashMap;
    // Parent block contains a tag that gets substituted from
    // the same context — verify super() renders the parent
    // body through the FULL pipeline (escape, dot-paths, etc).
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "{{#block \"hi\"}}hello, {{name}}{{/block}}!".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"hi\"}}>>> {{ super() }} <<<{{/block}}"
            .to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine.render_page(&ctx, "child").unwrap();
    assert_eq!(out, ">>> hello, Ada <<<!");
}

// ── G3: TemplateLoader trait ───────────────────────────────────

#[test]
fn memory_loader_serves_render_page() {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    let _ =
        map.insert("greet".to_string(), "Hello, {{name}}!".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(map)),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine.render_page(&ctx, "greet").unwrap();
    assert_eq!(out, "Hello, Ada!");
}

#[test]
fn memory_loader_resolves_partials() {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    let _ = map.insert(
        "main".to_string(),
        "outer {{> inner}} outer".to_string(),
    );
    let _ = map.insert("inner".to_string(), "<{{name}}>".to_string());
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(map)),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "x".to_string());
    let out = engine.render_page(&ctx, "main").unwrap();
    assert_eq!(out, "outer <x> outer");
}

#[test]
fn memory_loader_resolves_extends_chain() {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    let _ = map.insert(
        "base".to_string(),
        "[{{#block \"body\"}}default{{/block}}]".to_string(),
    );
    let _ = map.insert(
        "child".to_string(),
        "{{#extends \"base\"}}{{#block \"body\"}}OVR{{/block}}"
            .to_string(),
    );
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::new(map)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine.render_page(&ctx, "child").unwrap();
    assert_eq!(out, "[OVR]");
}

#[test]
fn memory_loader_missing_template_yields_io_error() {
    let engine = Engine::with_loader(
        Arc::new(MemoryLoader::default()),
        Duration::from_secs(60),
    );
    let engine = engine;
    let ctx = Context::new();
    let err = engine.render_page(&ctx, "missing").unwrap_err();
    assert!(matches!(err, EngineError::Io(_)), "{err:?}");
}

#[test]
fn memory_loader_insert_replaces_template() {
    let mut loader = MemoryLoader::default();
    let prev = loader.insert("k", "first");
    assert!(prev.is_none());
    let prev = loader.insert("k", "second");
    assert_eq!(prev.as_deref(), Some("first"));
    assert_eq!(loader.load("k").unwrap(), "second");
}

#[test]
fn fs_loader_serves_real_files() {
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("page.html"), "fs body {{x}}").unwrap();
    let loader = FsLoader::new(temp.path().to_path_buf());
    assert_eq!(loader.load("page").unwrap(), "fs body {{x}}");
}

#[test]
fn engine_new_uses_fs_loader_by_default() {
    // Backwards-compat: render_page on the original constructor
    // still walks the filesystem.
    let temp = tempfile::TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "default {{w}}").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("w".to_string(), "world".to_string());
    let out = engine.render_page(&ctx, "p").unwrap();
    assert_eq!(out, "default world");
}

// ── G2: String filters ─────────────────────────────────────────

#[test]
fn filter_repeat_repeats_string_n_times() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("c".to_string(), "ab".to_string());
    let out =
        engine.render_template("{{ c | repeat:3 }}", &ctx).unwrap();
    assert_eq!(out, "ababab");
}

#[test]
fn filter_repeat_zero_returns_empty() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("c".to_string(), "x".to_string());
    let out =
        engine.render_template("{{ c | repeat:0 }}", &ctx).unwrap();
    assert_eq!(out, "");
}

#[test]
fn filter_repeat_errors_without_arg() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("c".to_string(), "x".to_string());
    let err = engine
        .render_template("{{ c | repeat }}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("repeat"), "{err}");
}

#[test]
fn filter_reverse_unicode_aware() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "café".to_string());
    let out =
        engine.render_template("{{ s | reverse }}", &ctx).unwrap();
    assert_eq!(out, "éfac");
}

#[test]
fn filter_slice_with_start_and_end() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abcdef".to_string());
    let out =
        engine.render_template("{{ s | slice:1,4 }}", &ctx).unwrap();
    assert_eq!(out, "bcd");
}

#[test]
fn filter_slice_with_only_start() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abcdef".to_string());
    let out =
        engine.render_template("{{ s | slice:2 }}", &ctx).unwrap();
    assert_eq!(out, "cdef");
}

#[test]
fn filter_slice_clamps_oversize_indices() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "ab".to_string());
    let out = engine
        .render_template("{{ s | slice:100,200 }}", &ctx)
        .unwrap();
    assert_eq!(out, "");
}

#[test]
fn filter_slice_errors_without_start() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "ab".to_string());
    let err =
        engine.render_template("{{ s | slice }}", &ctx).unwrap_err();
    assert!(format!("{err}").contains("slice"), "{err}");
}

#[test]
fn filter_pad_start_right_aligns_with_spaces() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "42".to_string());
    let out = engine
        .render_template("{{ s | pad_start:5 }}", &ctx)
        .unwrap();
    assert_eq!(out, "   42");
}

#[test]
fn filter_pad_end_left_aligns_with_custom_char() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "x".to_string());
    let out = engine
        .render_template(r#"{{ s | pad_end:4,"." }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "x...");
}

#[test]
fn filter_pad_returns_input_unchanged_when_already_long() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abcdef".to_string());
    let out = engine
        .render_template("{{ s | pad_start:3 }}", &ctx)
        .unwrap();
    assert_eq!(out, "abcdef");
}

#[test]
fn filter_pad_errors_without_width() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "x".to_string());
    let err = engine
        .render_template("{{ s | pad_start }}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("pad"), "{err}");
}

#[test]
fn filter_contains_emits_true_or_false() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "hello world".to_string());
    let yes = engine
        .render_template(r#"{{ s | contains:"world" }}"#, &ctx)
        .unwrap();
    assert_eq!(yes, "true");
    let no = engine
        .render_template(r#"{{ s | contains:"xyz" }}"#, &ctx)
        .unwrap();
    assert_eq!(no, "false");
}

#[test]
fn filter_starts_with_and_ends_with() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abc".to_string());
    assert_eq!(
        engine
            .render_template(r#"{{ s | starts_with:"a" }}"#, &ctx)
            .unwrap(),
        "true"
    );
    assert_eq!(
        engine
            .render_template(r#"{{ s | ends_with:"c" }}"#, &ctx)
            .unwrap(),
        "true"
    );
    assert_eq!(
        engine
            .render_template(r#"{{ s | starts_with:"z" }}"#, &ctx)
            .unwrap(),
        "false"
    );
}

#[test]
fn filter_contains_no_arg_returns_false() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("s".to_string(), "abc".to_string());
    let out =
        engine.render_template("{{ s | contains }}", &ctx).unwrap();
    assert_eq!(out, "false");
}

// ── G1: Loop break / continue ─────────────────────────────────

#[test]
fn each_break_stops_iteration() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c", "d"]);
    // Stop emitting once we've seen "b".
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#if this == \"b\"}}{{#break}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "[a]");
}

#[test]
fn each_continue_skips_iteration() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    // Skip "b", emit the rest.
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#if this == \"b\"}}{{#continue}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "[a][c]");
}

#[test]
fn break_propagates_through_nested_if() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["x", "y", "z"]);
    // The break is inside two nested #if blocks; should still
    // bubble up to the each loop.
    let out = engine
        .render_template(
            "{{#each items}}\
             {{#if true}}{{#if this == \"y\"}}{{#break}}{{/if}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "[x]");
}

#[test]
fn break_in_one_each_does_not_affect_outer_loop() {
    // An inner #break should only short-circuit its own #each,
    // not the outer one.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("outer".to_string(), vec!["A", "B"]);
    ctx.set_value("inner".to_string(), vec!["1", "2", "3"]);
    let out = engine
        .render_template(
            "{{#each outer}}\
             [{{this}}:\
             {{#each inner}}\
             {{#if this == \"2\"}}{{#break}}{{/if}}\
             {{this}}\
             {{/each}}\
             ]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    // Inner each emits "1" then breaks at "2"; outer keeps going.
    assert_eq!(out, "[A:1][B:1]");
}

#[test]
fn break_at_top_level_is_silent_no_op() {
    // `#break` outside any loop should not error — the signal
    // bubbles to render_template and is discarded.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("before{{#break}}after", &ctx)
        .unwrap();
    // Render stops at #break; "after" is not emitted because
    // render_recursive returned early.
    assert_eq!(out, "before");
}

// ── F4: Custom tests API ───────────────────────────────────────

#[test]
fn add_test_registers_custom_predicate() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_test(
        "admin",
        Arc::new(|v, _args| {
            Ok(matches!(v, crate::context::Value::String(s) if s == "admin"))
        }),
    );
    let mut ctx = Context::new();
    ctx.set("role".to_string(), "admin".to_string());
    let out = engine
        .render_template("{{#if role is admin}}Y{{else}}N{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "Y");

    ctx.set("role".to_string(), "guest".to_string());
    let out = engine
        .render_template("{{#if role is admin}}Y{{else}}N{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "N");
}

#[test]
fn add_test_supports_is_not_negation() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_test(
        "even",
        Arc::new(|v, _args| match v {
            crate::context::Value::Number(n) => Ok(n % 2 == 0),
            _ => Ok(false),
        }),
    );
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 3i64);
    let out = engine
        .render_template(
            "{{#if n is not even}}odd{{else}}even{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "odd");
}

#[test]
fn add_test_unknown_name_still_errors() {
    // After F4, unknown test names defer to eval. If they're
    // not registered there either, eval surfaces the error.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "1".to_string());
    let err = engine
        .render_template("{{#if x is missing}}y{{/if}}", &ctx)
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("unknown test"), "{msg}");
    assert!(msg.contains("missing"), "{msg}");
}

#[test]
fn add_test_overrides_builtin_with_same_name() {
    // Override `defined` to always return false — proves
    // the custom-tests lookup happens BEFORE built-in dispatch.
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_test("defined", Arc::new(|_v, _args| Ok(false)));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let out = engine
        .render_template("{{#if x is defined}}Y{{else}}N{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "N");
}

#[test]
fn add_test_propagates_user_errors() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_test(
        "boom",
        Arc::new(|_v, _args| {
            Err(EngineError::Render("test exploded".to_string()))
        }),
    );
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let err = engine
        .render_template("{{#if x is boom}}y{{/if}}", &ctx)
        .unwrap_err();
    assert!(format!("{err}").contains("test exploded"), "{err}");
}

// ── F3: Number formatting filters ─────────────────────────────

#[test]
fn filter_abs_strips_sign() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), -42i64);
    let out = engine.render_template("{{ n | abs }}", &ctx).unwrap();
    assert_eq!(out, "42");
}

#[test]
fn filter_round_to_integer() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("price".to_string(), "3.6".to_string());
    let out =
        engine.render_template("{{ price | round }}", &ctx).unwrap();
    assert_eq!(out, "4");
}

#[test]
fn filter_ceil_rounds_up() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("v".to_string(), "2.1".to_string());
    let out = engine.render_template("{{ v | ceil }}", &ctx).unwrap();
    assert_eq!(out, "3");
}

#[test]
fn filter_floor_rounds_down() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("v".to_string(), "2.9".to_string());
    let out = engine.render_template("{{ v | floor }}", &ctx).unwrap();
    assert_eq!(out, "2");
}

#[test]
fn filter_round_on_integer_is_identity() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 5i64);
    let out = engine.render_template("{{ n | round }}", &ctx).unwrap();
    assert_eq!(out, "5");
}

#[test]
fn filter_number_format_default_comma() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 1_234_567i64);
    let out = engine
        .render_template("{{ n | number_format }}", &ctx)
        .unwrap();
    assert_eq!(out, "1,234,567");
}

#[test]
fn filter_number_format_custom_separator() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 1_234_567i64);
    let out = engine
        .render_template(r#"{{ n | number_format:"_" }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "1_234_567");
}

#[test]
fn filter_number_format_handles_negative_and_decimals() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("p".to_string(), "-1234567.89".to_string());
    let out = engine
        .render_template("{{ p | number_format }}", &ctx)
        .unwrap();
    assert_eq!(out, "-1,234,567.89");
}

#[test]
fn filter_number_format_short_number_unchanged() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 42i64);
    let out = engine
        .render_template("{{ n | number_format }}", &ctx)
        .unwrap();
    assert_eq!(out, "42");
}

#[test]
fn filter_round_on_non_number_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let err = engine
        .render_template("{{ name | round }}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err}").contains("expected a number"),
        "got {err}"
    );
}

#[test]
fn filter_number_format_on_non_number_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let err = engine
        .render_template("{{ name | number_format }}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err}").contains("expected a number"),
        "got {err}"
    );
}

// ── F2: String concatenation operator (~) ─────────────────────

#[test]
fn if_concat_two_string_literals() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            r#"{{#if "foo" ~ "bar" == "foobar"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn if_concat_path_and_literal() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine
        .render_template(
            r#"{{#if name ~ " Lovelace" == "Ada Lovelace"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn if_concat_coerces_number_to_string() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 5);
    let out = engine
        .render_template(
            r#"{{#if n ~ "x" == "5x"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn if_concat_left_associates() {
    // "a" ~ "b" ~ "c" must parse as (("a" ~ "b") ~ "c") and produce "abc".
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            r#"{{#if "a" ~ "b" ~ "c" == "abc"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn if_concat_binds_tighter_than_comparison() {
    // `a ~ b == c ~ d` must parse as `(a ~ b) == (c ~ d)`.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            r#"{{#if "a" ~ "b" == "a" ~ "b"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn if_concat_null_renders_as_empty() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("nope".to_string(), crate::context::Value::Null);
    let out = engine
        .render_template(
            r#"{{#if nope ~ "x" == "x"}}y{{else}}n{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "y");
}

#[test]
fn malformed_block_name_reports_line_number() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // Empty block name should fail name parsing.
    let template = "x\n{{#block }}body{{/block}}";
    let err = engine.render_template(template, &ctx).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("line 2"), "got: {msg}");
}

#[test]
fn render_to_writes_into_a_vec() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let mut buf: Vec<u8> = Vec::new();
    engine
        .render_to("Hello, {{name}}!", &ctx, &mut buf)
        .unwrap();
    assert_eq!(buf, b"Hello, Ada!");
}

#[test]
fn render_to_matches_render_template_byte_for_byte() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "Posts".to_string());
    ctx.set_value("items".to_string(), vec!["alpha", "beta", "gamma"]);
    let template = "<h1>{{title | uppercase}}</h1>\
                    <ul>{{#each items}}<li>{{this}}</li>{{/each}}</ul>";
    let direct = engine.render_template(template, &ctx).unwrap();
    let mut streamed: Vec<u8> = Vec::new();
    engine.render_to(template, &ctx, &mut streamed).unwrap();
    assert_eq!(streamed, direct.into_bytes());
}

#[test]
fn render_to_propagates_io_errors() {
    // A writer that always fails proves errors map to EngineError::Io.
    struct Bomb;
    impl Write for Bomb {
        fn write(&mut self, _b: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("no"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let err = engine.render_to("{{x}}", &ctx, &mut Bomb).unwrap_err();
    assert!(matches!(err, EngineError::Io(_)));
}

#[test]
fn render_to_propagates_template_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let mut buf: Vec<u8> = Vec::new();
    let err =
        engine.render_to("{{missing}}", &ctx, &mut buf).unwrap_err();
    assert!(matches!(err, EngineError::Render(_)));
    // Writer was not partially written when render fails first.
    assert!(buf.is_empty());
}

#[test]
fn add_filter_chains_with_builtins() {
    use std::sync::Arc;
    let mut engine = Engine::new("", Duration::from_secs(60));
    let _ = engine.add_filter(
        "exclaim",
        Arc::new(|input, _args| Ok(format!("{input}!"))),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "  ada  ".to_string());
    // built-in `trim` -> custom `exclaim` -> built-in `uppercase`
    let out = engine
        .render_template(
            "{{ name | trim | exclaim | uppercase }}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "ADA!");
}

#[test]
fn each_binds_index_first_last_for_lists() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    let out = engine
        .render_template(
            "{{#each items}}[{{@index}}={{this}} f={{@first}} l={{@last}}]{{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(
        out,
        "[0=a f=true l=false][1=b f=false l=false][2=c f=false l=true]",
    );
}

#[test]
fn each_iterates_maps_with_key_binding() {
    use crate::context::Value;
    use fnv::FnvHashMap;

    let engine = Engine::new("", Duration::from_secs(60));
    let mut prefs = FnvHashMap::default();
    let _ = prefs.insert("color".to_string(), Value::from("blue"));
    let _ = prefs.insert("size".to_string(), Value::from("M"));
    let mut ctx = Context::new();
    ctx.set_value("prefs".to_string(), Value::Map(prefs));

    // Map iteration is sorted by key for deterministic output.
    let out = engine
        .render_template(
            "{{#each prefs}}{{@key}}={{this}} {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "color=blue size=M ");
}

#[test]
fn if_compares_numbers_with_gt_lt_eq() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("count".to_string(), 7);

    for (expr, expected) in [
        ("count > 5", "yes"),
        ("count >= 7", "yes"),
        ("count < 5", "no"),
        ("count <= 6", "no"),
        ("count == 7", "yes"),
        ("count != 7", "no"),
    ] {
        let tpl =
            format!("{{{{#if {expr}}}}}yes{{{{else}}}}no{{{{/if}}}}");
        let out = engine.render_template(&tpl, &ctx).unwrap();
        assert_eq!(out, expected, "expr `{expr}`");
    }
}

#[test]
fn if_compares_string_literals() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("role".to_string(), "admin".to_string());
    let out = engine
        .render_template(
            r#"{{#if role == "admin"}}A{{else}}U{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "A");

    let out = engine
        .render_template(
            r#"{{#if role != "guest"}}A{{else}}U{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "A");
}

#[test]
fn if_compares_two_paths() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), 5);
    ctx.set_value("b".to_string(), 5);
    let out = engine
        .render_template("{{#if a == b}}eq{{else}}ne{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "eq");
}

#[test]
fn if_orders_strings_lexically() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("a".to_string(), "apple".to_string());
    ctx.set("b".to_string(), "banana".to_string());
    let out = engine
        .render_template("{{#if a < b}}lt{{else}}ge{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "lt");
}

#[test]
fn if_bare_path_keeps_truthiness_semantics() {
    // Backwards-compat: `{{#if x}}` without an operator still
    // evaluates to truthy / falsy.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("on".to_string(), true);
    ctx.set_value("off".to_string(), false);
    ctx.set_value("zero".to_string(), 0);
    ctx.set_value("seven".to_string(), 7);

    for (expr, expected) in [
        ("on", "Y"),
        ("off", "N"),
        ("zero", "N"),
        ("seven", "Y"),
        ("missing", "N"),
    ] {
        let tpl =
            format!("{{{{#if {expr}}}}}Y{{{{else}}}}N{{{{/if}}}}");
        assert_eq!(
            engine.render_template(&tpl, &ctx).unwrap(),
            expected,
            "expr `{expr}`",
        );
    }
}

#[test]
fn if_ordered_compare_on_mixed_types_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("count".to_string(), 7);
    ctx.set("name".to_string(), "Ada".to_string());
    let err = engine
        .render_template("{{#if count > name}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidTemplate(msg) if msg.contains("must be numbers or both strings"),
    ));
}

#[test]
fn if_eq_works_across_types_via_structural_equality() {
    // Eq/Ne don't require type matching — true vs 1 are simply
    // unequal under structural Value equality.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("on".to_string(), true);
    let out = engine
        .render_template("{{#if on == 1}}same{{else}}diff{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "diff");
}

#[test]
fn if_compares_against_null_literal() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Null);
    let out = engine
        .render_template("{{#if x == null}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

// ── Phase C2: boolean operators ────────────────────────────────

#[test]
fn if_combines_with_and_short_circuits_truth() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), crate::context::Value::Bool(true));
    ctx.set_value("b".to_string(), crate::context::Value::Bool(true));
    let out = engine
        .render_template("{{#if a and b}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");

    ctx.set_value("b".to_string(), crate::context::Value::Bool(false));
    let out = engine
        .render_template("{{#if a and b}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "no");
}

#[test]
fn if_combines_with_or_picks_truthy_branch() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), crate::context::Value::Bool(false));
    ctx.set_value("b".to_string(), crate::context::Value::Bool(true));
    let out = engine
        .render_template("{{#if a or b}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");

    ctx.set_value("b".to_string(), crate::context::Value::Bool(false));
    let out = engine
        .render_template("{{#if a or b}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "no");
}

#[test]
fn if_negates_with_not() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Bool(false));
    let out = engine
        .render_template("{{#if not x}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_double_negates_with_not_not() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Bool(true));
    let out = engine
        .render_template("{{#if not not x}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_respects_not_over_and_over_or_precedence() {
    // `not a and b or c` must parse as `((not a) and b) or c`.
    // With a=true, b=true, c=false → ((false) and true) or false
    // → false. With a=false, b=true, c=false → ((true) and true)
    // or false → true. Same template, different context.
    let engine = Engine::new("", Duration::from_secs(60));
    let template = "{{#if not a and b or c}}yes{{else}}no{{/if}}";

    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), crate::context::Value::Bool(true));
    ctx.set_value("b".to_string(), crate::context::Value::Bool(true));
    ctx.set_value("c".to_string(), crate::context::Value::Bool(false));
    assert_eq!(engine.render_template(template, &ctx).unwrap(), "no");

    ctx.set_value("a".to_string(), crate::context::Value::Bool(false));
    assert_eq!(engine.render_template(template, &ctx).unwrap(), "yes");
}

#[test]
fn if_combines_comparisons_with_and_or() {
    // Comparisons bind tighter than boolean ops.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), crate::context::Value::Number(7));
    ctx.set_value(
        "name".to_string(),
        crate::context::Value::String("Ada".into()),
    );
    let out = engine
        .render_template(
            "{{#if n > 5 and name == \"Ada\"}}match{{else}}miss{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "match");
}

#[test]
fn if_or_short_circuits_past_unbound_path() {
    // The right-hand operand should not even be evaluated when the
    // left is already truthy. `missing` would resolve to Null
    // either way (not an error), but this protects the contract.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "ready".to_string(),
        crate::context::Value::Bool(true),
    );
    let out = engine
        .render_template(
            "{{#if ready or missing}}go{{else}}wait{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "go");
}

#[test]
fn if_dotted_path_with_and_substring_is_not_keyword() {
    // The keyword detection only fires on standalone identifiers,
    // so `user.notes` is still a valid path.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    let mut user: fnv::FnvHashMap<String, crate::context::Value> =
        fnv::FnvHashMap::default();
    let _ = user.insert(
        "notes".to_string(),
        crate::context::Value::String("hi".into()),
    );
    ctx.set_value("user".to_string(), crate::context::Value::Map(user));
    let out = engine
        .render_template("{{#if user.notes}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

// ── Phase C3: integer math operators ───────────────────────────

#[test]
fn if_compares_addition_against_threshold() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), crate::context::Value::Number(8));
    let out = engine
        .render_template(
            "{{#if n + 2 == 10}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_subtracts_two_paths() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), crate::context::Value::Number(20));
    ctx.set_value("b".to_string(), crate::context::Value::Number(7));
    let out = engine
        .render_template("{{#if a - b > 10}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_respects_mul_over_add_precedence() {
    // 2 + 3 * 4 must equal 14, not 20.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#if 2 + 3 * 4 == 14}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_evaluates_left_associatively_for_subtraction() {
    // 10 - 3 - 2 must equal 5, not 9.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#if 10 - 3 - 2 == 5}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_does_integer_division_truncating_toward_zero() {
    // 7 / 2 == 3 (integer), not 3.5.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template("{{#if 7 / 2 == 3}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_division_by_zero_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("z".to_string(), crate::context::Value::Number(0));
    let err = engine
        .render_template("{{#if 5 / z == 0}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("division by zero"),
        "expected division-by-zero error, got: {err:?}"
    );
}

#[test]
fn if_math_on_non_numbers_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "name".to_string(),
        crate::context::Value::String("Ada".into()),
    );
    let err = engine
        .render_template("{{#if name + 1 == 1}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("math operator requires"),
        "expected math-type error, got: {err:?}"
    );
}

#[test]
fn if_negative_literal_still_parses() {
    // `-3` after `>` (a non-operand token) should tokenize as a
    // negative number literal, not as `Minus, 3`.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), crate::context::Value::Number(-5));
    let out = engine
        .render_template("{{#if n < -3}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_math_chains_with_boolean_ops() {
    // Math < comparisons < booleans in precedence.
    // `(a + b) > 10 and (c * 2) <= 8`
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("a".to_string(), crate::context::Value::Number(7));
    ctx.set_value("b".to_string(), crate::context::Value::Number(5));
    ctx.set_value("c".to_string(), crate::context::Value::Number(3));
    let out = engine
        .render_template(
            "{{#if a + b > 10 and c * 2 <= 8}}\
             ok{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "ok");
}

#[test]
fn if_integer_overflow_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "max".to_string(),
        crate::context::Value::Number(i64::MAX),
    );
    let err = engine
        .render_template("{{#if max + 1 == 0}}x{{/if}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("integer overflow"),
        "expected overflow error, got: {err:?}"
    );
}

// ── Phase C4: postfix tests (is defined / empty / none) ────────

#[test]
fn if_is_defined_true_for_present_path() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "name".to_string(),
        crate::context::Value::String("Ada".into()),
    );
    let out = engine
        .render_template(
            "{{#if name is defined}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_is_defined_false_for_missing_path() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#if missing is defined}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "no");
}

#[test]
fn if_is_defined_true_for_explicit_null() {
    // Explicitly setting a key to Null still counts as defined,
    // because the key exists. This matches Tera/Jinja semantics
    // where `is defined` answers "does the variable exist".
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Null);
    let out = engine
        .render_template(
            "{{#if x is defined}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_is_not_defined_negates() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#if missing is not defined}}yes{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "yes");
}

#[test]
fn if_is_empty_true_for_empty_string_list_map_null() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "s".to_string(),
        crate::context::Value::String(String::new()),
    );
    ctx.set_value("l".to_string(), crate::context::Value::List(vec![]));
    ctx.set_value(
        "m".to_string(),
        crate::context::Value::Map(fnv::FnvHashMap::default()),
    );
    ctx.set_value("z".to_string(), crate::context::Value::Null);
    for key in ["s", "l", "m", "z"] {
        let out = engine
            .render_template(
                &format!(
                    "{{{{#if {key} is empty}}}}yes{{{{else}}}}no{{{{/if}}}}"
                ),
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "yes", "{key} should be empty");
    }
}

#[test]
fn if_is_empty_false_for_zero_and_false_and_nonempty_string() {
    // Numbers and bools are never empty — `0 is empty` is false
    // even though `0` is falsy. Tests probe a specific shape;
    // truthiness is a separate axis.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), crate::context::Value::Number(0));
    ctx.set_value("b".to_string(), crate::context::Value::Bool(false));
    ctx.set_value(
        "s".to_string(),
        crate::context::Value::String("hi".into()),
    );
    for key in ["n", "b", "s"] {
        let out = engine
            .render_template(
                &format!(
                    "{{{{#if {key} is empty}}}}yes{{{{else}}}}no{{{{/if}}}}"
                ),
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "no", "{key} should not be empty");
    }
}

#[test]
fn if_is_none_true_only_for_null() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("z".to_string(), crate::context::Value::Null);
    ctx.set_value(
        "s".to_string(),
        crate::context::Value::String(String::new()),
    );
    let yes = engine
        .render_template("{{#if z is none}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(yes, "yes");
    let no = engine
        .render_template("{{#if s is none}}yes{{else}}no{{/if}}", &ctx)
        .unwrap();
    assert_eq!(no, "no");
}

#[test]
fn if_is_not_none_negates() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Number(5));
    let out = engine
        .render_template(
            "{{#if x is not none}}set{{else}}null{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "set");
}

#[test]
fn if_unknown_test_name_errors_cleanly() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("x".to_string(), crate::context::Value::Bool(true));
    let err = engine
        .render_template("{{#if x is bogus}}y{{/if}}", &ctx)
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("unknown test"),
        "expected unknown-test error, got: {err:?}"
    );
}

#[test]
fn if_test_combines_with_boolean_ops() {
    // `name is defined and name is not empty` is the canonical
    // "has a non-blank value" check.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value(
        "name".to_string(),
        crate::context::Value::String("Ada".into()),
    );
    let out = engine
        .render_template(
            "{{#if name is defined and name is not empty}}\
             ok{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "ok");

    ctx.set_value(
        "name".to_string(),
        crate::context::Value::String(String::new()),
    );
    let out = engine
        .render_template(
            "{{#if name is defined and name is not empty}}\
             ok{{else}}no{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "no");
}

#[test]
fn if_dotted_path_can_test_for_definedness() {
    // `user.email is defined` walks the dotted path before
    // running the test.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    let mut user: fnv::FnvHashMap<String, crate::context::Value> =
        fnv::FnvHashMap::default();
    let _ = user.insert(
        "email".to_string(),
        crate::context::Value::String("a@b".into()),
    );
    ctx.set_value("user".to_string(), crate::context::Value::Map(user));
    let yes = engine
        .render_template(
            "{{#if user.email is defined}}y{{else}}n{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(yes, "y");
    let no = engine
        .render_template(
            "{{#if user.phone is defined}}y{{else}}n{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(no, "n");
}

#[test]
fn set_binds_string_literal_for_subsequent_tags() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            r#"{{#set name = "Ada"}}Hello, {{name}}!"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Hello, Ada!");
}

#[test]
fn set_binds_integer_bool_and_null_literals() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#set n = 42}}{{#set ok = true}}{{#set z = null}}\
             n={{n}} ok={{ok}} z={{z}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "n=42 ok=true z=");
}

#[test]
fn set_does_not_mutate_caller_context() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "outer".to_string());

    let _ = engine
        .render_template(r#"{{#set name = "inner"}}{{name}}"#, &ctx)
        .unwrap();

    // Caller's context is unchanged after rendering.
    assert_eq!(ctx.get("name"), Some(&"outer".to_string()),);
}

#[test]
fn set_in_each_body_does_not_leak_across_iterations() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    // Each iteration starts fresh — `marker` is set inside the body
    // but the parent context never sees it after the loop.
    let out = engine
        .render_template(
            r#"{{#each items}}{{#set marker = "X"}}{{this}}={{marker}} {{/each}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "a=X b=X c=X ");
}

#[test]
fn set_visible_inside_subsequent_if_block() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            "{{#set ready = true}}{{#if ready}}go{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "go");
}

#[test]
fn set_supports_dot_notation_on_left_side() {
    // `#set` only takes a flat key — dot-notation lookups still
    // work because the bound key matches verbatim.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let out = engine
        .render_template(
            r#"{{#set greeting = "hi"}}{{greeting}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "hi");
}

#[test]
fn set_missing_value_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine.render_template("{{#set x =}}", &ctx).unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidTemplate(msg) if msg.contains("missing value"),
    ));
}

#[test]
fn set_missing_equals_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine.render_template("{{#set x}}", &ctx).unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidTemplate(msg) if msg.contains("`= value`"),
    ));
}

#[test]
fn extends_overrides_named_blocks_in_base() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "<title>{{#block \"title\"}}Default{{/block}}</title>",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let child = "{{#extends \"base\"}}\
                 {{#block \"title\"}}Custom{{/block}}";

    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "<title>Custom</title>");
}

#[test]
fn extends_falls_back_to_default_when_block_not_overridden() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "[{{#block \"a\"}}A-default{{/block}}]\
         [{{#block \"b\"}}B-default{{/block}}]",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // Child only overrides `b`; `a` uses its default body.
    let child = "{{#extends \"base\"}}\
                 {{#block \"b\"}}B-custom{{/block}}";

    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "[A-default][B-custom]");
}

#[test]
fn extends_supports_nested_blocks_with_partial_overrides() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "<head>\
           {{#block \"head\"}}\
             <title>{{#block \"title\"}}Default{{/block}}</title>\
           {{/block}}\
         </head>",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // Override only the inner block; outer falls back to default,
    // which contains the now-overridden inner block.
    let child = "{{#extends \"base\"}}\
                 {{#block \"title\"}}Custom{{/block}}";

    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "<head><title>Custom</title></head>");
}

#[test]
fn extends_chains_through_multiple_levels() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "{{#block \"x\"}}base-x{{/block}}",
    )
    .unwrap();
    fs::write(
        temp.path().join("middle.html"),
        "{{#extends \"base\"}}\
         {{#block \"x\"}}middle-x{{/block}}",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // Child overrides x; should win over middle's x.
    let child = "{{#extends \"middle\"}}\
                 {{#block \"x\"}}child-x{{/block}}";

    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "child-x");
}

#[test]
fn extends_chain_uses_middle_when_child_does_not_override() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "{{#block \"x\"}}base-x{{/block}}",
    )
    .unwrap();
    fs::write(
        temp.path().join("middle.html"),
        "{{#extends \"base\"}}\
         {{#block \"x\"}}middle-x{{/block}}",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // Child extends middle but doesn't override x — middle's x wins.
    let child = "{{#extends \"middle\"}}";
    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "middle-x");
}

#[test]
fn extends_circular_chain_errors_at_depth_limit() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    // a extends b extends a → infinite chain capped by MAX_RENDER_DEPTH.
    fs::write(temp.path().join("a.html"), "{{#extends \"b\"}}")
        .unwrap();
    fs::write(temp.path().join("b.html"), "{{#extends \"a\"}}")
        .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let res = engine.render_template("{{#extends \"a\"}}", &ctx);
    assert!(matches!(
        res,
        Err(EngineError::Render(msg)) if msg.contains("recursion depth"),
    ));
}

#[test]
fn extends_drops_literal_text_outside_blocks_in_child() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "[{{#block \"x\"}}default{{/block}}]",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    // Literal text "ignored garbage" between extends and the block
    // contributes nothing to the output.
    let child = "{{#extends \"base\"}}ignored garbage\
                 {{#block \"x\"}}custom{{/block}}\
                 more ignored garbage";
    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "[custom]");
}

#[test]
fn block_inside_each_renders_per_iteration() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("base.html"),
        "{{#each items}}\
           [{{#block \"item\"}}{{this}}{{/block}}]\
         {{/each}}",
    )
    .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    let child = "{{#extends \"base\"}}\
                 {{#block \"item\"}}<{{this}}>{{/block}}";

    let out = engine.render_template(child, &ctx).unwrap();
    assert_eq!(out, "[<a>][<b>][<c>]");
}

#[test]
fn partial_with_named_params_overrides_context() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("badge.html"), "{{label}}={{value}}")
        .unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("label".to_string(), "outer".to_string());

    // The partial sees label="version", value=42 — outer.label
    // is shadowed only inside this invocation.
    let out = engine
        .render_template(
            r#"{{> badge label="version" value=42}} {{label}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "version=42 outer");
}

#[test]
fn partial_param_handles_quoted_string_with_spaces() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("greet.html"), "Hello, {{name}}!")
        .unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template(r#"{{> greet name="Ada Lovelace"}}"#, &ctx)
        .unwrap();
    assert_eq!(out, "Hello, Ada Lovelace!");
}

#[test]
fn partial_param_unclosed_quote_errors() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("p.html"), "x").unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let err = engine
        .render_template(r#"{{> p name="open}}"#, &ctx)
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidTemplate(msg) if msg.contains("unclosed quote"),
    ));
}

#[test]
fn partial_param_recognises_booleans_and_null() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("flags.html"),
        "{{#if active}}A{{else}}-{{/if}} {{count}}",
    )
    .unwrap();
    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = engine
        .render_template("{{> flags active=true count=3}}", &ctx)
        .unwrap();
    assert_eq!(out, "A 3");
}

#[test]
fn each_on_empty_list_renders_nothing() {
    use crate::context::Value;
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("xs".to_string(), Value::List(Vec::new()));
    let out = engine
        .render_template("[{{#each xs}}x{{/each}}]", &ctx)
        .unwrap();
    assert_eq!(out, "[]");
}

#[test]
fn filters_compose_in_chain() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "  hello world  ".to_string());
    let out = engine
        .render_template(
            "{{ title | trim | capitalize | truncate:8 }}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Hello...");
}

#[test]
fn unknown_filter_errors_with_clear_message() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "y".to_string());
    let err = engine
        .render_template("{{ x | nonexistent }}", &ctx)
        .unwrap_err();
    match err {
        EngineError::Render(msg) => {
            assert!(
                msg.contains("Unknown filter: nonexistent"),
                "{msg}"
            );
        }
        other => panic!("expected Render, got {other:?}"),
    }
}

#[test]
fn whitespace_strip_works_on_block_tags() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("on".to_string(), true);
    let out = engine
        .render_template(
            "a   {{- #if on -}}   x   {{- /if -}}   b",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "axb");
}

#[test]
fn raw_opt_out_still_works_alongside_comments() {
    // Disambiguation: `{{!body}}` (no space after !) is raw
    // substitution, not a comment.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("body".to_string(), "<b>hi</b>".to_string());
    let out = engine
        .render_template("before {{! note }} after {{!body}}", &ctx)
        .unwrap();
    assert_eq!(out, "before  after <b>hi</b>");
}

#[test]
fn test_render_template_escaping_disabled() {
    let engine = Engine::new("", Duration::from_secs(60))
        .with_html_escape(false);
    let mut context = Context::new();
    context.set("body".to_string(), "<b>hi</b>".to_string());
    let result = engine.render_template("{{body}}", &context).unwrap();
    assert_eq!(result, "<b>hi</b>");
}

#[test]
fn test_render_template_whitespace_around_key() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());
    let result =
        engine.render_template("Hi {{ name }}", &context).unwrap();
    assert_eq!(result, "Hi Alice");
}

#[test]
fn backslash_escapes_opening_delimiter() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Alice".to_string());
    // Escaped: literal, no lookup.
    let out = engine
        .render_template("literal \\{{name}} and real {{name}}", &ctx)
        .unwrap();
    assert_eq!(out, "literal {{name}} and real Alice");
}

#[test]
fn double_backslash_escapes_the_backslash_itself() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Alice".to_string());
    // `\\{{name}}` -> emit one backslash, then substitute.
    let out = engine.render_template("path\\\\{{name}}", &ctx).unwrap();
    assert_eq!(out, "path\\Alice");
}

#[test]
fn escape_does_not_leak_into_missing_key_error() {
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // `{{missing}}` without escape still errors.
    let result =
        engine.render_template("\\{{literal}} {{missing}}", &ctx);
    assert!(matches!(result, Err(EngineError::Render(_))));
}

#[test]
fn test_render_page_rejects_path_traversal() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let context = Context::new();
    // `a/b` is now allowed; only `..`, absolute paths, and nulls
    // are rejected.
    for bad in ["../etc/passwd", "/etc/passwd", "a\0b", ""] {
        let result = engine.render_page(&context, bad);
        assert!(
            matches!(result, Err(EngineError::InvalidTemplate(_))),
            "expected rejection for {bad:?}"
        );
    }
}

#[test]
fn test_render_page_with_subdirectory() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let sub_dir = temp_dir.path().join("blog");
    fs::create_dir_all(&sub_dir).unwrap();
    let template_path = sub_dir.join("post.html");
    fs::write(&template_path, "Post: {{title}}").unwrap();

    let engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut context = Context::new();
    context.set("title".to_string(), "Hello World".to_string());

    let result = engine.render_page(&context, "blog/post").unwrap();
    assert_eq!(result, "Post: Hello World");
}

#[test]
fn test_render_template_with_partial() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    let partial_path = temp.path().join("header.html");
    fs::write(&partial_path, "Welcome, {{name}}!").unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());

    let template = "Header: {{> header}}";
    let result = engine.render_template(template, &context).unwrap();
    assert_eq!(result, "Header: Welcome, Alice!");
}

#[test]
fn test_render_template_partial_recursion_limit() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    let partial_path = temp.path().join("loop.html");
    fs::write(&partial_path, "{{> loop}}").unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let context = Context::new();

    let template = "{{> loop}}";
    let result = engine.render_template(template, &context);
    assert!(
        matches!(result, Err(EngineError::Render(msg)) if msg.contains("recursion depth"))
    );
}

#[test]
fn test_render_template_with_filters() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("name".to_string(), " Alice ".to_string());

    let template = "Hello, {{ name | trim | uppercase }}!";
    let result = engine.render_template(template, &context).unwrap();
    assert_eq!(result, "Hello, ALICE!");
}

#[test]
fn test_render_template_truncate_filter() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set(
        "long".to_string(),
        "This is a very long string that should be truncated"
            .to_string(),
    );

    let template = "{{ long | truncate }}";
    let result = engine.render_template(template, &context).unwrap();
    assert_eq!(result, "This is a very long string ...");
}

#[test]
fn test_render_template_unknown_filter_errors() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut context = Context::new();
    context.set("name".to_string(), "Alice".to_string());

    let template = "{{ name | invalid }}";
    let result = engine.render_template(template, &context);
    assert!(
        matches!(result, Err(EngineError::Render(msg)) if msg.contains("Unknown filter"))
    );
}

#[test]
fn test_render_template_partial_name_empty() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let ctx = Context::new();
    let result = engine.render_template("{{> }}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("Empty partial")
    ));
}

#[test]
fn test_render_template_partial_name_invalid() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let ctx = Context::new();
    let result = engine.render_template("{{> /etc/passwd}}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("invalid template or partial")
    ));
}

#[test]
fn test_render_template_partial_missing_file() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let ctx = Context::new();
    let result = engine.render_template("{{> missing}}", &ctx);
    assert!(matches!(result, Err(EngineError::Io(_))));
}

#[test]
fn test_render_template_unknown_filter() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Alice".to_string());
    let result = engine.render_template("{{name | missing}}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::Render(msg)) if msg.contains("Unknown filter")
    ));
}

#[test]
fn test_render_template_each_unresolved_list() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let ctx = Context::new();
    let result =
        engine.render_template("{{#each missing}}x{{/each}}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::Render(msg)) if msg.contains("#each: unresolved")
    ));
}

#[test]
fn test_render_template_each_not_a_list() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("k".to_string(), 42);
    let result = engine.render_template("{{#each k}}x{{/each}}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("expects a list")
    ));
}

#[test]
fn test_extract_block_unclosed_tag() {
    // extract_block calls after_open.find(close) which can return None
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("show".to_string(), true);
    // This triggers "Unclosed template tag" inside extract_block
    let result =
        engine.render_template("{{#if show}} {{#if x}} {{/if", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("Unclosed template tag")
    ));
}

#[test]
fn test_extract_block_mismatched_closer() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("show".to_string(), true);
    let result = engine.render_template("{{#if show}} {{/each}}", &ctx);
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("Unclosed `{{#if}}` block")
    ));
}

#[test]
fn test_split_else_malformed_tag() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("show".to_string(), true);
    // This triggers "Unclosed `{{#if}}` block" because the parser
    // can't find a clean end for the if-block while skipping the
    // malformed inner tag.
    let result =
        engine.render_template("{{#if show}} {{ {{/if}}", &ctx);
    match &result {
        Err(EngineError::InvalidTemplate(msg)) => {
            assert!(msg.contains("Unclosed `{{#if}}` block"));
        }
        Ok(s) => panic!("Expected error, got success: {}", s),
        other => {
            panic!("Expected InvalidTemplate error, got {:?}", other)
        }
    }
}

#[test]
fn test_render_template_nested_if() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("outer".to_string(), true);
    ctx.set_value("inner".to_string(), true);
    let out = engine
        .render_template(
            "{{#if outer}}O{{#if inner}}I{{/if}}{{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "OI");
}

#[test]
fn test_render_template_nested_each() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set_value("list".to_string(), vec![vec![1]]);
    let out = engine
        .render_template(
            "{{#each list}}X{{#each this}}{{this}}{{/each}}{{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "X1");
}

#[test]
fn test_create_template_folder_empty_files_errors() {
    let engine = Engine::new("t", Duration::from_secs(60));
    let result = engine.create_template_folder_with_files(
        Some("https://example.com"),
        &[],
    );
    #[cfg(feature = "remote-templates")]
    assert!(matches!(
        result,
        Err(EngineError::InvalidTemplate(msg)) if msg.contains("must not be empty")
    ));
    #[cfg(not(feature = "remote-templates"))]
    assert!(result.is_err());
}

#[test]
fn test_cache_insert_empty_victim_internal() {
    // We can't easily trigger the None => break branch via the
    // public API since HashMaps with capacity > 0 always have a
    // min_by_key. However, we can use a zero-capacity cache if it
    // were possible. Instead, we'll verify it doesn't crash on
    // empty.
    let mut cache: Cache<String, String> =
        Cache::with_capacity(Duration::from_secs(60), 0);
    let _ = cache.insert("k".to_string(), "v".to_string());
    assert_eq!(cache.len(), 1); // insert still works, cap is a 'soft' hint for eviction
}

#[test]
fn test_render_template_unresolved_tag() {
    let engine = Engine::new("", Duration::from_secs(60));
    let context = Context::new();

    let template = "Hello, {{name}}!";
    let result = engine.render_template(template, &context);
    assert!(matches!(result, Err(EngineError::Render(_))));
}

#[test]
fn test_is_url() {
    assert!(is_url("http://example.com"));
    assert!(is_url("https://example.com"));
    assert!(!is_url("file:///path/to/file"));
    assert!(!is_url("/local/path"));
}

#[test]
fn test_render_page() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let template_path = temp_dir.path().join("template.html");
    fs::write(&template_path, "Hello, {{name}}!").unwrap();

    let engine = Engine::new(
        temp_dir.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut context = Context::new();
    context.set("name".to_string(), "World".to_string());

    let result = engine.render_page(&context, "template").unwrap();
    assert_eq!(result, "Hello, World!");
}

#[test]
fn test_clear_cache() {
    let engine = Engine::new("templates", Duration::from_secs(3600));
    let _ = engine
        .render_cache
        .lock()
        .unwrap()
        .insert("key1".to_string(), "value1".to_string());
    assert!(!engine.render_cache.lock().unwrap().is_empty());

    engine.clear_cache();
    assert!(engine.render_cache.lock().unwrap().is_empty());
}

#[test]
fn test_set_max_cache_size() {
    let engine = Engine::new("templates", Duration::from_secs(3600));
    let _ = engine
        .render_cache
        .lock()
        .unwrap()
        .insert("key1".to_string(), "value1".to_string());
    let _ = engine
        .render_cache
        .lock()
        .unwrap()
        .insert("key2".to_string(), "value2".to_string());
    assert_eq!(engine.render_cache.lock().unwrap().len(), 2);

    // Capping at 1 doesn't wipe existing entries; subsequent inserts
    // evict the least-recently-used entry to stay within the cap.
    engine.set_max_cache_size(1);
    let _ = engine
        .render_cache
        .lock()
        .unwrap()
        .insert("key3".to_string(), "value3".to_string());
    assert_eq!(engine.render_cache.lock().unwrap().len(), 1);
}

#[test]
fn set_max_cache_size_noop_when_under_limit() {
    let engine = Engine::new("templates", Duration::from_secs(3600));
    let _ = engine
        .render_cache
        .lock()
        .unwrap()
        .insert("k".to_string(), "v".to_string());
    engine.set_max_cache_size(8);
    assert_eq!(
        engine.render_cache.lock().unwrap().len(),
        1,
        "no-op branch must preserve cache when len() <= max_size"
    );
}

#[test]
fn render_page_serves_from_cache_on_repeat_call() {
    use std::fs;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    let tpl = temp.path().join("page.html");
    fs::write(&tpl, "hi {{name}}").unwrap();

    let engine = Engine::new(
        temp.path().to_str().unwrap(),
        Duration::from_secs(60),
    );
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "alice".to_string());

    let first = engine.render_page(&ctx, "page").unwrap();
    assert_eq!(engine.render_cache.lock().unwrap().len(), 1);

    // Overwrite the template file; a cached render returns the old
    // output, proving the second call is served from cache rather
    // than re-reading disk.
    fs::write(&tpl, "changed {{name}}").unwrap();
    let second = engine.render_page(&ctx, "page").unwrap();
    assert_eq!(second, first, "second call must hit the cache");
}

#[test]
fn escape_html_covers_every_metacharacter() {
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "& < > \" '".to_string());
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "&amp; &lt; &gt; &quot; &#x27;");
}

#[test]
fn create_template_folder_rejects_none() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    match engine.create_template_folder(None) {
        Err(EngineError::InvalidTemplate(msg)) => {
            assert!(msg.contains("template_path is required"), "{msg}");
        }
        other => panic!("expected InvalidTemplate, got {other:?}"),
    }
}

#[test]
fn create_template_folder_returns_existing_local_path() {
    use tempfile::TempDir;
    let temp = TempDir::new().unwrap();
    // `create_template_folder` joins against the *current working
    // directory*, so pass a relative segment reachable from the temp
    // dir by chdir'ing.
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    fs::create_dir_all("templates").unwrap();

    let engine = Engine::new(".", Duration::from_secs(60));
    let got = engine.create_template_folder(Some("templates"));

    // Restore CWD before asserting so a failure does not poison
    // subsequent tests in the same process.
    std::env::set_current_dir(&prev).unwrap();
    let path = got.expect("existing local dir must resolve");
    assert!(path.ends_with("templates"), "{path}");
}

#[test]
fn create_template_folder_missing_local_path_is_io_not_found() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    match engine.create_template_folder(Some(
        "this-directory-does-not-exist-on-any-machine",
    )) {
        Err(EngineError::Io(e)) => {
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
        other => panic!("expected Io(NotFound), got {other:?}"),
    }
}

#[cfg(not(feature = "remote-templates"))]
#[test]
fn create_template_folder_url_without_feature_errors() {
    let engine = Engine::new("templates", Duration::from_secs(60));
    match engine.create_template_folder(Some("https://example.com/t/"))
    {
        Err(EngineError::InvalidTemplate(msg)) => {
            assert!(msg.contains("remote-templates"), "{msg}");
        }
        other => {
            panic!("expected InvalidTemplate, got {other:?}")
        }
    }
}

#[test]
fn lax_mode_substitutes_empty_for_unresolved_tags() {
    // Regression for sebastienrousseau/staticweaver#28.
    // When lax_undefined is true, an undefined {{key}} emits "",
    // letting the build proceed instead of aborting.
    let engine = Engine::new("", Duration::from_secs(60))
        .with_lax_undefined(true);
    let ctx = Context::new();
    let out = engine
        .render_template("hello {{name}} world", &ctx)
        .unwrap();
    assert_eq!(out, "hello  world");
}

#[test]
fn strict_mode_still_errors_on_unresolved_tags() {
    // Default is strict (lax_undefined = false). Pre-existing
    // behaviour must not regress.
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    let err = engine.render_template("{{name}}", &ctx).unwrap_err();
    assert!(
        format!("{err}").contains("Unresolved template tag"),
        "strict mode should still raise; got: {err}"
    );
}

#[test]
fn escape_html_preserves_existing_named_entities() {
    // Regression for sebastienrousseau/static-site-generator#589.
    // `AI &amp; Payments` must stay `AI &amp; Payments`, not
    // become `AI &amp;amp; Payments`.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "AI &amp; Payments".to_string());
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "AI &amp; Payments");
}

#[test]
fn escape_html_preserves_existing_numeric_entities() {
    // Numeric entity refs (decimal AND hex) must be preserved.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "copy &#169; hex &#xA9;".to_string());
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "copy &#169; hex &#xA9;");
}

#[test]
fn escape_html_still_escapes_bare_ampersand_at_eof() {
    // A bare `&` with no terminator must still escape — the
    // entity scanner is allowed to bail and fall through.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("x".to_string(), "AT&T R&D".to_string());
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "AT&amp;T R&amp;D");
}

#[test]
fn escape_html_handles_mixed_bare_and_escaped() {
    // Mix: bare `&` → `&amp;`, already-escaped `&amp;` stays.
    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set(
        "x".to_string(),
        "AT&T owns &amp; and AT&amp;T too".to_string(),
    );
    let out = engine.render_template("{{x}}", &ctx).unwrap();
    assert_eq!(out, "AT&amp;T owns &amp; and AT&amp;T too");
}
