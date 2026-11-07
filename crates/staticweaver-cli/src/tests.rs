// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! In-process unit tests for the CLI (see the comment in `main.rs`).

use super::*;
use std::io::Cursor;

fn s(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_string()).collect()
}

#[test]
fn empty_args_emits_help_with_exit_2() {
    let out = dispatch(&[], &mut Cursor::new(Vec::new()));
    assert_eq!(out.exit, 2);
    assert!(out.stderr.contains("USAGE"));
    assert!(out.stdout.is_empty());
}

#[test]
fn help_long_short_and_subcommand_form() {
    for arg in ["--help", "-h", "help"] {
        let out = dispatch(&s(&[arg]), &mut Cursor::new(Vec::new()));
        assert_eq!(out.exit, 0);
        assert!(out.stdout.contains("USAGE"));
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn version_long_and_short() {
    for arg in ["--version", "-V"] {
        let out = dispatch(&s(&[arg]), &mut Cursor::new(Vec::new()));
        assert_eq!(out.exit, 0);
        assert!(out.stdout.contains("staticweaver"));
        assert!(out.stdout.contains(VERSION));
    }
}

#[test]
fn unknown_subcommand_errors_with_exit_2() {
    let out = dispatch(&s(&["nope"]), &mut Cursor::new(Vec::new()));
    assert_eq!(out.exit, 2);
    assert!(out.stderr.contains("unknown subcommand"));
}

#[test]
fn render_with_stdin_template_and_set() {
    let out = dispatch(
        &s(&["render", "-", "--set", "name=Ada"]),
        &mut Cursor::new(b"Hi {{name}}!".to_vec()),
    );
    assert_eq!(out.exit, 0);
    assert_eq!(out.stdout, "Hi Ada!");
}

#[test]
fn render_with_no_escape_emits_raw_html() {
    let out = dispatch(
        &s(&["render", "-", "--no-escape", "--set", "x=<b>"]),
        &mut Cursor::new(b"{{x}}".to_vec()),
    );
    assert_eq!(out.exit, 0);
    assert_eq!(out.stdout, "<b>");
}

#[test]
fn render_default_escapes_html() {
    let out = dispatch(
        &s(&["render", "-", "--set", "x=<b>"]),
        &mut Cursor::new(b"{{x}}".to_vec()),
    );
    assert_eq!(out.exit, 0);
    assert_eq!(out.stdout, "&lt;b&gt;");
}

#[test]
fn render_from_file_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("t.html");
    std::fs::write(&path, "<h1>{{title}}</h1>").unwrap();
    let out = dispatch(
        &s(&["render", path.to_str().unwrap(), "--set", "title=Hi"]),
        &mut Cursor::new(Vec::new()),
    );
    assert_eq!(out.exit, 0);
    assert_eq!(out.stdout, "<h1>Hi</h1>");
}

#[test]
fn render_missing_template_arg_errors() {
    let out = dispatch(&s(&["render"]), &mut Cursor::new(Vec::new()));
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("missing"));
}

#[test]
fn render_malformed_set_errors() {
    let out = dispatch(
        &s(&["render", "-", "--set", "no_equals"]),
        &mut Cursor::new(Vec::new()),
    );
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("KEY=VALUE"));
}

#[test]
fn render_set_without_value_errors() {
    let out = dispatch(
        &s(&["render", "-", "--set"]),
        &mut Cursor::new(Vec::new()),
    );
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("--set requires"));
}

#[test]
fn render_unknown_arg_errors() {
    let out = dispatch(
        &s(&["render", "-", "--never-heard-of"]),
        &mut Cursor::new(Vec::new()),
    );
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("unknown arg"));
}

#[test]
fn watch_flag_is_parsed_and_routed() {
    let parsed =
        parse_render_args(&s(&["page.html", "--watch"])).unwrap();
    assert!(parsed.watch);
    assert!(watch_request(&s(&["render", "page.html", "--watch"]))
        .is_some());
    // Without --watch, or with a malformed command line, the
    // ordinary dispatch path handles (and reports) it.
    assert!(watch_request(&s(&["render", "page.html"])).is_none());
    assert!(
        watch_request(&s(&["render", "--watch", "--bogus"])).is_none()
    );
    assert!(watch_request(&s(&["--watch"])).is_none());
    assert!(HELP.contains("--watch"));
}

#[test]
fn render_template_error_propagates() {
    let out = dispatch(
        &s(&["render", "-"]),
        &mut Cursor::new(b"{{missing}}".to_vec()),
    );
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("missing"));
}

#[test]
fn render_nonexistent_file_errors() {
    let out = dispatch(
        &s(&["render", "/nonexistent-staticweaver-cli-path"]),
        &mut Cursor::new(Vec::new()),
    );
    assert_eq!(out.exit, 1);
    assert!(out.stderr.contains("reading template"));
}

#[test]
fn parse_render_args_roundtrips_clean_input() {
    let parsed = parse_render_args(&s(&[
        "t.html",
        "--set",
        "a=1",
        "--no-escape",
        "--set",
        "b=2",
    ]))
    .unwrap();
    assert_eq!(parsed.template_arg.as_deref(), Some("t.html"));
    assert_eq!(
        parsed.sets,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string())
        ]
    );
    assert!(parsed.no_escape);
}

#[test]
fn parse_render_args_treats_dash_as_template() {
    let parsed = parse_render_args(&s(&["-"])).unwrap();
    assert_eq!(parsed.template_arg.as_deref(), Some("-"));
}

// ── run() entry point: covers main's terminal-write branches

#[test]
fn run_help_writes_to_stdout_and_exits_zero() {
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit =
        run(s(&["--help"]), &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(exit, 0);
    let s_out = String::from_utf8(stdout).unwrap();
    assert!(s_out.contains("USAGE"), "{s_out}");
    assert!(stderr.is_empty());
}

#[test]
fn run_unknown_subcommand_writes_to_stderr_with_exit_2() {
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = run(s(&["nope"]), &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    let s_err = String::from_utf8(stderr).unwrap();
    assert!(s_err.contains("unknown subcommand"), "{s_err}");
}

#[test]
fn run_render_writes_template_output_to_stdout() {
    let mut stdin = Cursor::new(b"Hi {{name}}!".to_vec());
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = run(
        s(&["render", "-", "--set", "name=Ada"]),
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(exit, 0);
    assert_eq!(stdout, b"Hi Ada!");
    assert!(stderr.is_empty());
}

#[test]
fn run_render_error_writes_to_stderr_with_exit_1() {
    let mut stdin = Cursor::new(b"{{missing}}".to_vec());
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit =
        run(s(&["render", "-"]), &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(exit, 1);
    assert!(stdout.is_empty());
    let s_err = String::from_utf8(stderr).unwrap();
    assert!(s_err.contains("missing"), "{s_err}");
}

#[test]
fn run_empty_args_writes_help_to_stderr_with_exit_2() {
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = run(s(&[]), &mut stdin, &mut stdout, &mut stderr);
    assert_eq!(exit, 2);
    assert!(stdout.is_empty());
    let s_err = String::from_utf8(stderr).unwrap();
    assert!(s_err.contains("USAGE"), "{s_err}");
}
