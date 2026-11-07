// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Unit tests for the crate-private parser and filter helpers. The
//! render behaviour itself is covered through the public API in the
//! `staticweaver` facade's test suite.

use crate::error::EngineError;
use crate::expr_parse::parse_expr;
use crate::filters::*;
use crate::path::validate_name;
use crate::syntax::*;
use std::io;

#[test]
fn parse_filter_recognises_quoted_commas() {
    let (name, args) = parse_filter(r#"replace:"a, b","c""#);
    assert_eq!(name, "replace");
    assert_eq!(args, vec!["a, b".to_string(), "c".to_string()]);
}

#[test]
fn parse_filter_handles_no_args() {
    let (name, args) = parse_filter("uppercase");
    assert_eq!(name, "uppercase");
    assert!(args.is_empty());
}

#[test]
fn pos_suffix_empty_for_unrelated_slice() {
    // A slice that isn't a substring of `origin` returns "".
    let origin = "alpha";
    let other = String::from("beta");
    assert_eq!(pos_suffix(origin, &other), "");
}

#[test]
fn annotate_pos_passes_through_io_errors() {
    // Only InvalidTemplate / Render get position-stamped; Io
    // (and other variants) flow through unchanged.
    let err = EngineError::Io(io::Error::other("raw"));
    let template = "abc";
    let wrapped = annotate_pos(err, template, &template[..1]);
    match wrapped {
        EngineError::Io(_) => {}
        other => panic!("expected Io, got {other:?}"),
    }
}

#[test]
fn annotate_pos_is_idempotent() {
    // A message that already carries `at line N, column M` is
    // not stamped twice.
    let err =
        EngineError::Render("boom at line 1, column 2".to_string());
    let template = "abc";
    let wrapped = annotate_pos(err, template, &template[..1]);
    if let EngineError::Render(msg) = wrapped {
        // Substring "at line " appears exactly once.
        let count = msg.matches("at line ").count();
        assert_eq!(count, 1, "double-stamped: {msg}");
    } else {
        panic!("expected Render variant");
    }
}

#[test]
fn filter_round_keeps_fractional_when_op_returns_non_integer() {
    // op = identity-on-fraction; result keeps the decimal.
    // We don't have a built-in op that returns a fractional
    // float, so exercise the format branch via parse_number_filter
    // directly with a custom op.
    let s = parse_number_filter("3", "abs", |n| n + 0.5).unwrap();
    assert_eq!(s, "3.5");
}

#[test]
fn parse_operand_errors_at_end_of_expression() {
    // Empty expression has no operand at all.
    let err = parse_expr("").unwrap_err();
    assert!(format!("{err}").contains("end of expression"), "{err}");
}

#[test]
fn parse_operand_errors_on_keyword_in_operand_position() {
    // Starting an expression with `and` is a parse error.
    let err = parse_expr("and 1").unwrap_err();
    assert!(format!("{err}").contains("expected operand"), "{err}");
}

#[test]
fn is_test_errors_on_missing_name() {
    // `x is` with nothing after is a parse error.
    let err = parse_expr("x is").unwrap_err();
    assert!(format!("{err}").contains("expected test name"), "{err}");
}

#[test]
fn is_test_errors_on_non_path_token_after_is() {
    // `x is 5` — the test name must be one of defined/empty/none,
    // and 5 isn't a Path token at all.
    let err = parse_expr("x is 5").unwrap_err();
    assert!(format!("{err}").contains("expected test name"), "{err}");
}

#[test]
fn annotate_pos_stamps_bare_render_message() {
    // A Render error without existing position info gets the
    // suffix appended.
    let err = EngineError::Render("plain message".to_string());
    let template = "abcdef";
    let wrapped = annotate_pos(err, template, &template[3..]);
    if let EngineError::Render(msg) = wrapped {
        assert!(msg.contains("plain message"), "{msg}");
        assert!(msg.contains("at line 1"), "{msg}");
        assert!(msg.contains("column 4"), "{msg}");
    } else {
        panic!("expected Render");
    }
}

#[test]
fn annotate_pos_stamps_bare_invalid_template_message() {
    let err = EngineError::InvalidTemplate("syntax oops".to_string());
    let template = "x";
    let wrapped = annotate_pos(err, template, &template[..1]);
    if let EngineError::InvalidTemplate(msg) = wrapped {
        assert!(msg.contains("syntax oops"), "{msg}");
        assert!(msg.contains("at line 1"), "{msg}");
    } else {
        panic!("expected InvalidTemplate");
    }
}

#[test]
fn parse_block_name_accepts_quoted_names() {
    // Exercises the quoted-strip return in parse_block_name.
    assert_eq!(parse_block_name("\"title\"").unwrap(), "title");
    assert_eq!(parse_block_name("'foo'").unwrap(), "foo");
}

#[test]
fn parse_expr_errors_on_trailing_junk() {
    // Exercises the "unexpected token in expression" path in
    // parse_expr — a complete expression followed by extra
    // tokens.
    let err = parse_expr("1 == 1 trailing").unwrap_err();
    assert!(format!("{err}").contains("unexpected token"), "{err}");
}

#[test]
fn parse_filter_args_handles_quoted_commas() {
    // Exercises the in-quote path of parse_filter_args (commas
    // inside quotes don't split) plus the comma-separator path.
    let args = parse_filter_args(r#""a,b","c""#);
    assert_eq!(args, vec!["a,b", "c"]);
}

#[test]
fn unknown_byte_in_expression_consumed_silently() {
    // Tokenizer encountering an unknown byte advances past it
    // and returns None for that scan; the parser then hits
    // end-of-expression.
    let err = parse_expr("?;%").unwrap_err();
    // Just verify it errors cleanly rather than panicking.
    assert!(
        matches!(err, EngineError::InvalidTemplate(_)),
        "got {err:?}"
    );
}

// ── annotate_pos branches

#[test]
fn annotate_pos_on_unrelated_slice_returns_err_unchanged() {
    // Synthetic slice that isn't part of `origin` → suffix is
    // empty and the early-return `if suffix.is_empty()` arm
    // fires (line ~2095).
    let origin = "abc";
    let synthetic = String::from("xyz");
    let err = EngineError::Render("boom".to_string());
    let wrapped = annotate_pos(err, origin, &synthetic);
    // No position info appended.
    assert_eq!(format!("{wrapped}"), "Render error: boom");
}

#[test]
fn block_name_accepts_bareword_and_quoted_forms() {
    // `parse_block_name` strips matching single/double quotes and
    // returns the inner. Bareword passes through.
    assert_eq!(parse_block_name("title").unwrap(), "title");
    assert_eq!(parse_block_name("\"title\"").unwrap(), "title");
    assert_eq!(parse_block_name("'title'").unwrap(), "title");
    assert!(parse_block_name("").is_err());
}

#[test]
fn test_validate_path_cases() {
    assert!(validate_name("").is_err());
    assert!(validate_name("/absolute").is_err());
    // A leading backslash is a root on Windows, a filename
    // character elsewhere.
    assert_eq!(validate_name("\\absolute").is_err(), cfg!(windows));
    assert!(validate_name("null\0char").is_err());
    // `..` is refused only when it climbs above the root.
    assert!(validate_name("blog/../etc/passwd").is_ok());
    assert!(validate_name("blog/../../etc/passwd").is_err());
    assert!(validate_name("blog/..").is_err());
    assert!(validate_name("blog/post").is_ok());
}

mod renderer_api {
    use crate::{
        Context, FilterFn, Registry, Renderer, TemplateLoader, TestFn,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn store() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        let _ =
            m.insert("page".to_string(), "<p>{{x}}</p>".to_string());
        m
    }

    fn ctx() -> Context {
        let mut c = Context::new();
        c.set("x".to_string(), "a&b".to_string());
        c
    }

    #[test]
    fn render_named_loads_validates_and_renders() {
        let loader = store();
        let r = Renderer::new(&loader);
        assert_eq!(
            r.render_named("page", &ctx()).unwrap(),
            "<p>a&amp;b</p>"
        );
        assert!(r.render_named("../page", &ctx()).is_err());
        assert!(r.render_named("missing", &ctx()).is_err());
    }

    #[test]
    fn btreemap_loader_reports_missing_as_not_found() {
        let err = store().load("nope").unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn render_to_writes_into_fmt_sink() {
        let loader = store();
        let mut out = String::new();
        Renderer::new(&loader)
            .escape_html(false)
            .render_to("{{x}}", &ctx(), &mut out)
            .unwrap();
        assert_eq!(out, "a&b");
    }

    struct Refuse;
    impl core::fmt::Write for Refuse {
        fn write_str(&mut self, _: &str) -> core::fmt::Result {
            Err(core::fmt::Error)
        }
    }

    #[test]
    fn render_to_reports_a_refused_write() {
        let loader = store();
        let err = Renderer::new(&loader)
            .render_to("x", &ctx(), &mut Refuse)
            .unwrap_err();
        assert!(err.to_string().contains("sink"), "{err}");
    }

    #[test]
    fn btreemap_registries_supply_filters_and_tests() {
        let loader = store();
        let mut filters: BTreeMap<String, FilterFn> = BTreeMap::new();
        let _ = filters
            .insert("twice".into(), Arc::new(|s, _| Ok(s.repeat(2))));
        let mut tests: BTreeMap<String, TestFn> = BTreeMap::new();
        let _ = tests.insert("yes".into(), Arc::new(|_, _| Ok(true)));
        assert!(filters.lookup("twice").is_some());
        let r = Renderer::new(&loader)
            .delimiters("[[", "]]")
            .lax_undefined(true)
            .filters(&filters)
            .tests(&tests);
        let out = r
            .render(
                "[[ x | twice ]][[#if x is yes]]!/[[/if]][[gone]]",
                &ctx(),
            )
            .unwrap();
        assert_eq!(out, "a&amp;ba&amp;b!/");
    }

    #[test]
    fn debug_lists_configuration_not_extensions() {
        let loader = store();
        let shown = format!("{:?}", Renderer::new(&loader));
        assert!(
            shown.contains("Renderer") && shown.contains("{{"),
            "{shown}"
        );
    }
}
