// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Output-stability snapshot tests.
//!
//! Each test pins the *exact* string that `render_template` /
//! `render_page` produces for a representative scenario covering
//! a major feature surface. If a refactor subtly changes whitespace,
//! escape behaviour, list ordering, or block resolution, one of
//! these will fail with a clear diff — much faster to diagnose than
//! a unit test that only checks "contains" or "not empty".
//!
//! Snapshots live as inline `&str` constants (rather than separate
//! files) so reviewers see the expected output next to the
//! input — no second-window context-switching.

use staticweaver::context::Value;
use staticweaver::engine::MemoryLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

fn engine() -> Engine {
    Engine::new("", Duration::from_secs(60))
}

#[test]
fn snapshot_simple_substitution() {
    let mut ctx = Context::new();
    ctx.set("greeting".to_string(), "Hello".to_string());
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine()
        .render_template("{{greeting}}, {{name}}!", &ctx)
        .unwrap();
    assert_eq!(out, "Hello, Ada!");
}

#[test]
fn snapshot_html_escape_of_dangerous_input() {
    let mut ctx = Context::new();
    ctx.set(
        "user".to_string(),
        "<script>alert('x')</script>".to_string(),
    );
    let out = engine().render_template("Hi {{user}}", &ctx).unwrap();
    assert_eq!(
        out,
        "Hi &lt;script&gt;alert(&#x27;x&#x27;)&lt;/script&gt;"
    );
}

#[test]
fn snapshot_each_with_loop_helpers() {
    let mut ctx = Context::new();
    ctx.set_value("items".to_string(), vec!["alpha", "beta", "gamma"]);
    let out = engine()
        .render_template(
            "{{#each items}}\
             [{{@index}}={{this}}f={{@first}}l={{@last}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(
        out,
        "[0=alphaf=truel=false][1=betaf=falsel=false][2=gammaf=falsel=true]"
    );
}

#[test]
fn snapshot_each_over_range() {
    let ctx = Context::new();
    let out = engine()
        .render_template("{{#each 1..5}}{{this}}|{{/each}}", &ctx)
        .unwrap();
    assert_eq!(out, "1|2|3|4|");
}

#[test]
fn snapshot_if_with_complex_expression() {
    let mut ctx = Context::new();
    ctx.set_value("score".to_string(), 85i64);
    ctx.set("name".to_string(), "Ada".to_string());
    let out = engine()
        .render_template(
            "{{#if score > 70 and name == \"Ada\"}}\
             Pass\
             {{else}}\
             Fail\
             {{/if}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Pass");
}

#[test]
fn snapshot_string_concat_via_tilde() {
    let mut ctx = Context::new();
    ctx.set("first".to_string(), "Ada".to_string());
    ctx.set("last".to_string(), "Lovelace".to_string());
    let out = engine()
        .render_template(
            r#"{{#if first ~ " " ~ last == "Ada Lovelace"}}match{{else}}no{{/if}}"#,
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "match");
}

#[test]
fn snapshot_each_with_break_and_continue() {
    let mut ctx = Context::new();
    ctx.set_value(
        "items".to_string(),
        vec!["a", "skip", "b", "stop", "c"],
    );
    let cont = engine()
        .render_template(
            "{{#each items}}\
             {{#if this == \"skip\"}}{{#continue}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(cont, "[a][b][stop][c]");
    let brk = engine()
        .render_template(
            "{{#each items}}\
             {{#if this == \"stop\"}}{{#break}}{{/if}}\
             [{{this}}]\
             {{/each}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(brk, "[a][skip][b]");
}

#[test]
fn snapshot_inheritance_with_super() {
    let mut store = HashMap::new();
    let _ = store.insert(
        "base".to_string(),
        "<title>{{#block \"t\"}}default{{/block}}</title>".to_string(),
    );
    let _ = store.insert(
        "child".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"t\"}}({{ super() }}) overridden{{/block}}"
            .to_string(),
    );
    let mut e = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = e.render_page(&ctx, "child").unwrap();
    assert_eq!(out, "<title>(default) overridden</title>");
}

#[test]
fn snapshot_filter_pipeline_long() {
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "  staticweaver  ".to_string());
    let out = engine()
        .render_template(
            "{{ title | trim | uppercase | reverse }}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "REVAEWCITATS");
}

#[test]
fn snapshot_number_format_filter() {
    let mut ctx = Context::new();
    ctx.set_value("n".to_string(), 1_234_567i64);
    let out = engine()
        .render_template(r#"{{ n | number_format:"_" }}"#, &ctx)
        .unwrap();
    assert_eq!(out, "1_234_567");
}

#[test]
fn snapshot_dot_notation_walk() {
    let mut ctx = Context::new();
    let mut user: fnv::FnvHashMap<String, Value> =
        fnv::FnvHashMap::default();
    let _ =
        user.insert("name".to_string(), Value::String("Ada".into()));
    let _ = user.insert(
        "tags".to_string(),
        Value::List(vec![
            Value::String("rust".into()),
            Value::String("templates".into()),
        ]),
    );
    ctx.set_value("user".to_string(), Value::Map(user));
    let out = engine()
        .render_template(
            "{{user.name}} likes {{user.tags.0}} and {{user.tags.1}}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Ada likes rust and templates");
}

#[test]
fn snapshot_set_assignment() {
    let ctx = Context::new();
    let out = engine()
        .render_template(
            "{{#set greeting = \"Hi\"}}{{#set who = \"world\"}}\
             {{ greeting }}, {{ who }}!",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "Hi, world!");
}

#[test]
fn snapshot_whitespace_control_strips_correctly() {
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let out =
        engine().render_template("  {{- name -}}  ", &ctx).unwrap();
    assert_eq!(out, "Ada");
}

#[test]
fn snapshot_partial_with_parameters() {
    let mut store = HashMap::new();
    let _ = store
        .insert("row".to_string(), "[{{label}}={{value}}]".to_string());
    let e = Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    );
    let ctx = Context::new();
    let out = e
        .render_template(r#"{{> row label="user" value="ada"}}"#, &ctx)
        .unwrap();
    assert_eq!(out, "[user=ada]");
}

#[test]
fn snapshot_line_col_in_error_messages() {
    let ctx = Context::new();
    // Unresolved tag on line 2.
    let err = engine()
        .render_template("ok\n{{missing}}", &ctx)
        .unwrap_err();
    let msg = format!("{err}");
    assert_eq!(
        msg,
        "Render error: Unresolved template tag: missing at line 2, column 3"
    );
}

#[cfg(feature = "json")]
#[test]
fn snapshot_json_filter_full_value_tree() {
    let mut ctx = Context::new();
    let mut user: fnv::FnvHashMap<String, Value> =
        fnv::FnvHashMap::default();
    let _ =
        user.insert("name".to_string(), Value::String("Ada".into()));
    let _ = user.insert("age".to_string(), Value::Number(28));
    let _ = user.insert("admin".to_string(), Value::Bool(true));
    ctx.set_value("user".to_string(), Value::Map(user));
    let out = engine()
        .render_template("{{ user | json | safe }}", &ctx)
        .unwrap();
    // Map keys are sorted for deterministic output.
    assert_eq!(out, r#"{"admin":true,"age":28,"name":"Ada"}"#);
}
