// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential tests: render the same logical template and context
//! through both `staticweaver` and `minijinja`, then assert the
//! outputs match.
//!
//! The two engines have *different* syntax — Mustache (`{{tag}}`,
//! `{{#if x}}…{{/if}}`) vs Jinja (`{{ tag }}`, `{% if x %}…{% endif %}`)
//! — so each scenario carries a pair of templates. The contract is on
//! the *output*: given the same logical inputs, the rendered bytes
//! are byte-for-byte identical.
//!
//! Differential testing catches semantic drift on shared features:
//!   * variable substitution
//!   * HTML escape contract
//!   * if/else truthiness
//!   * each / for iteration
//!   * filter pipeline
//!
//! It also serves as a forcing function — if we ever diverge from
//! "same output for the same context", we want a failing test, not a
//! surprise in production.

use minijinja::Environment;
use staticweaver::{Context, Engine};
use std::time::Duration;

fn sw_engine() -> Engine {
    Engine::new("", Duration::from_secs(60))
}

#[test]
fn simple_substitution_matches_minijinja() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set("name".to_string(), "Ada".to_string());
    let sw_out =
        sw.render_template("Hello, {{name}}!", &sw_ctx).unwrap();

    let mut env = Environment::new();
    env.add_template("t", "Hello, {{ name }}!").unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { name => "Ada" })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn html_escape_matches_minijinja_for_shared_metachars() {
    // Minijinja additionally escapes `/` to `&#x2f;` as a
    // defense-in-depth mitigation against `</script>` injection;
    // staticweaver sticks to the 5-character OWASP set
    // (& < > " '). Test only the shared subset so the contract
    // we *do* match doesn't drift.
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set("body".to_string(), "a & b < c > \"d\" 'e'".to_string());
    let sw_out =
        sw.render_template("<p>{{body}}</p>", &sw_ctx).unwrap();

    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    env.add_template("t", "<p>{{ body }}</p>").unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { body => "a & b < c > \"d\" 'e'" })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn html_escape_diverges_from_minijinja_on_slash() {
    // Anchor test for the documented divergence — protects against
    // regressing into Minijinja's "escape /" behaviour by accident.
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set("p".to_string(), "/path".to_string());
    let sw_out = sw.render_template("{{p}}", &sw_ctx).unwrap();
    assert_eq!(
        sw_out, "/path",
        "staticweaver must NOT escape `/` (5-char OWASP set only)",
    );

    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    env.add_template("t", "{{ p }}").unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { p => "/path" })
        .unwrap();
    assert_eq!(
        mj_out, "&#x2f;path",
        "minijinja escapes `/` — keep these tests aware of that",
    );
}

#[test]
fn each_over_string_list_matches_minijinja() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx
        .set_value("items".to_string(), vec!["alpha", "beta", "gamma"]);
    let sw_out = sw
        .render_template("{{#each items}}[{{this}}]{{/each}}", &sw_ctx)
        .unwrap();

    let mut env = Environment::new();
    env.add_template(
        "t",
        "{% for item in items %}[{{ item }}]{% endfor %}",
    )
    .unwrap();
    let items: Vec<&str> = vec!["alpha", "beta", "gamma"];
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { items => items })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn if_else_matches_minijinja_for_truthy_path() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set_value("active".to_string(), true);
    let sw_out = sw
        .render_template("{{#if active}}YES{{else}}NO{{/if}}", &sw_ctx)
        .unwrap();

    let mut env = Environment::new();
    env.add_template("t", "{% if active %}YES{% else %}NO{% endif %}")
        .unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { active => true })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn if_else_matches_minijinja_for_falsy_path() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set_value("active".to_string(), false);
    let sw_out = sw
        .render_template("{{#if active}}YES{{else}}NO{{/if}}", &sw_ctx)
        .unwrap();

    let mut env = Environment::new();
    env.add_template("t", "{% if active %}YES{% else %}NO{% endif %}")
        .unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { active => false })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn uppercase_filter_matches_minijinja() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set("s".to_string(), "hello".to_string());
    let sw_out =
        sw.render_template("{{ s | uppercase }}", &sw_ctx).unwrap();

    let mut env = Environment::new();
    env.add_template("t", "{{ s | upper }}").unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { s => "hello" })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn dot_path_lookup_matches_minijinja() {
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    let mut user: fnv::FnvHashMap<
        String,
        staticweaver::context::Value,
    > = fnv::FnvHashMap::default();
    let _ = user.insert(
        "name".to_string(),
        staticweaver::context::Value::String("Ada".into()),
    );
    sw_ctx.set_value(
        "user".to_string(),
        staticweaver::context::Value::Map(user),
    );
    let sw_out =
        sw.render_template("hi {{user.name}}", &sw_ctx).unwrap();

    let mut env = Environment::new();
    env.add_template("t", "hi {{ user.name }}").unwrap();
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { user => minijinja::context! { name => "Ada" } })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}

#[test]
fn each_with_loop_index_matches_minijinja() {
    // We use @index (0-based) and minijinja exposes loop.index0 (also 0-based).
    let sw = sw_engine();
    let mut sw_ctx = Context::new();
    sw_ctx.set_value("items".to_string(), vec!["a", "b", "c"]);
    let sw_out = sw
        .render_template(
            "{{#each items}}{{@index}}={{this}};{{/each}}",
            &sw_ctx,
        )
        .unwrap();

    let mut env = Environment::new();
    env.add_template(
        "t",
        "{% for item in items %}{{ loop.index0 }}={{ item }};{% endfor %}",
    )
    .unwrap();
    let items: Vec<&str> = vec!["a", "b", "c"];
    let mj_out = env
        .get_template("t")
        .unwrap()
        .render(minijinja::context! { items => items })
        .unwrap();

    assert_eq!(sw_out, mj_out);
}
