// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Smoke tests for the `staticweaver` CLI binary.
//!
//! Exercises the binary built by `cargo`. Uses
//! `env!("CARGO_BIN_EXE_staticweaver")` to find the path the test
//! harness has cached, so no extra build invocations or path-juggling
//! is needed.

use std::io::Write as _;
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_staticweaver"))
}

#[test]
fn cli_help_prints_usage_to_stdout() {
    let out = bin().arg("--help").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USAGE"), "{stdout}");
    assert!(stdout.contains("render"), "{stdout}");
    assert!(stdout.contains("--set"), "{stdout}");
}

#[test]
fn cli_version_prints_version() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("staticweaver"), "{stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}

#[test]
fn cli_renders_template_from_stdin_with_set() {
    let mut child = bin()
        .args([
            "render",
            "-",
            "--set",
            "name=Ada",
            "--set",
            "greet=Hello",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{{greet}}, {{name}}!")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"Hello, Ada!");
}

#[test]
fn cli_renders_template_from_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("t.html");
    std::fs::write(&path, "<h1>{{title}}</h1>").unwrap();
    let out = bin()
        .args(["render", path.to_str().unwrap(), "--set", "title=Hi"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"<h1>Hi</h1>");
}

#[test]
fn cli_no_escape_emits_raw_html() {
    let out = bin()
        .args(["render", "-", "--no-escape", "--set", "x=<b>"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.as_mut().unwrap().write_all(b"{{x}}").unwrap();
            c.wait_with_output()
        })
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"<b>");
}

#[test]
fn cli_default_escapes_html() {
    let out = bin()
        .args(["render", "-", "--set", "x=<b>"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.as_mut().unwrap().write_all(b"{{x}}").unwrap();
            c.wait_with_output()
        })
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"&lt;b&gt;");
}

#[test]
fn cli_unknown_subcommand_exits_non_zero() {
    let out = bin().arg("nope").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown subcommand"), "{stderr}");
}

#[test]
fn cli_missing_template_exits_non_zero() {
    let out = bin().arg("render").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing"), "{stderr}");
}

#[test]
fn cli_malformed_set_arg_errors() {
    let out = bin()
        .args(["render", "-", "--set", "no_equals_here"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("KEY=VALUE"), "{stderr}");
}

#[test]
fn cli_render_error_exits_non_zero() {
    // Unresolved tag => Render error => exit 1.
    let out = bin()
        .args(["render", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin
                .as_mut()
                .unwrap()
                .write_all(b"{{missing}}")
                .unwrap();
            c.wait_with_output()
        })
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing"), "{stderr}");
}
