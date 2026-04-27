// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `staticweaver` CLI — render templates from the shell.
//!
//! ```text
//! USAGE
//!   staticweaver render <template> [--set key=value ...] [--no-escape]
//!   staticweaver --help
//!   staticweaver --version
//!
//! ARGS
//!   <template>          Template file path, or `-` to read from stdin.
//!
//! OPTIONS
//!   --set KEY=VALUE     Set a context binding. Repeatable.
//!   --no-escape         Disable HTML escape.
//!   --help, -h          Print this message.
//!   --version, -V       Print the binary version.
//!
//! EXAMPLES
//!   staticweaver render hello.html --set name=Ada
//!   echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
//! ```
//!
//! Hand-rolled arg parsing — no `clap` dep, so the binary stays
//! lightweight and `cargo install staticweaver` produces a small
//! executable.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use staticweaver::{Context, Engine};
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
USAGE
  staticweaver render <template> [--set key=value ...] [--no-escape]
  staticweaver --help
  staticweaver --version

ARGS
  <template>          Template file path, or `-` to read from stdin.

OPTIONS
  --set KEY=VALUE     Set a context binding. Repeatable.
  --no-escape         Disable HTML escape.
  --help, -h          Print this message.
  --version, -V       Print the binary version.

EXAMPLES
  staticweaver render hello.html --set name=Ada
  echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
";

/// Outcome of `dispatch` — used by both the binary's `main` and the
/// unit tests inside this file. Carrying stdout/stderr as separate
/// strings lets tests assert on each stream independently.
#[derive(Debug, Default, PartialEq, Eq)]
struct CliOutput {
    stdout: String,
    stderr: String,
    exit: u8,
}

/// Parsed `render` subcommand arguments. Pure data so the parser
/// is testable without a Reader.
#[derive(Debug, Default, PartialEq, Eq)]
struct RenderArgs {
    template_arg: Option<String>,
    sets: Vec<(String, String)>,
    no_escape: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = dispatch(&args, &mut std::io::stdin());
    if !out.stdout.is_empty() {
        print!("{}", out.stdout);
    }
    if !out.stderr.is_empty() {
        eprint!("{}", out.stderr);
    }
    ExitCode::from(out.exit)
}

/// Top-level subcommand dispatch. `stdin_reader` is plumbed so unit
/// tests can pass a `&[u8]` instead of inheriting the process stdin.
fn dispatch<R: Read>(
    args: &[String],
    stdin_reader: &mut R,
) -> CliOutput {
    if args.is_empty() {
        return CliOutput {
            stderr: format!("{HELP}\n"),
            exit: 2,
            ..CliOutput::default()
        };
    }
    if matches!(args[0].as_str(), "--help" | "-h" | "help") {
        return CliOutput {
            stdout: format!("{HELP}\n"),
            exit: 0,
            ..CliOutput::default()
        };
    }
    if matches!(args[0].as_str(), "--version" | "-V") {
        return CliOutput {
            stdout: format!("staticweaver {VERSION}\n"),
            exit: 0,
            ..CliOutput::default()
        };
    }
    if args[0] != "render" {
        return CliOutput {
            stderr: format!(
                "error: unknown subcommand `{}`\n\n{HELP}\n",
                args[0]
            ),
            exit: 2,
            ..CliOutput::default()
        };
    }
    match cmd_render(&args[1..], stdin_reader) {
        Ok(out) => CliOutput {
            stdout: out,
            exit: 0,
            ..CliOutput::default()
        },
        Err(msg) => CliOutput {
            stderr: format!("error: {msg}\n"),
            exit: 1,
            ..CliOutput::default()
        },
    }
}

fn parse_render_args(args: &[String]) -> Result<RenderArgs, String> {
    let mut out = RenderArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--set" => {
                let pair = args.get(i + 1).ok_or_else(|| {
                    "--set requires a KEY=VALUE argument".to_string()
                })?;
                let (k, v) = pair.split_once('=').ok_or_else(|| {
                    format!("--set expected KEY=VALUE, got `{pair}`")
                })?;
                out.sets.push((k.to_string(), v.to_string()));
                i += 2;
            }
            "--no-escape" => {
                out.no_escape = true;
                i += 1;
            }
            // `-` alone means "read template from stdin" — not an
            // option flag. Anything else starting with `-` is.
            other
                if (other == "-" || !other.starts_with('-'))
                    && out.template_arg.is_none() =>
            {
                out.template_arg = Some(other.to_string());
                i += 1;
            }
            other => {
                return Err(format!("unknown arg `{other}`"));
            }
        }
    }
    Ok(out)
}

fn cmd_render<R: Read>(
    args: &[String],
    stdin_reader: &mut R,
) -> Result<String, String> {
    let parsed = parse_render_args(args)?;

    let tmpl_src = parsed
        .template_arg
        .as_deref()
        .ok_or_else(|| "missing <template> argument".to_string())?;
    let template = if tmpl_src == "-" {
        let mut s = String::new();
        let _ = stdin_reader
            .read_to_string(&mut s)
            .map_err(|e| format!("reading stdin: {e}"))?;
        s
    } else {
        std::fs::read_to_string(tmpl_src).map_err(|e| {
            format!("reading template `{tmpl_src}`: {e}")
        })?
    };

    let mut ctx = Context::new();
    for (k, v) in parsed.sets {
        ctx.set(k, v);
    }

    let engine = Engine::new("", Duration::from_secs(60))
        .with_html_escape(!parsed.no_escape);
    engine
        .render_template(&template, &ctx)
        .map_err(|e| format!("render: {e}"))
}

// ─── Unit tests ─────────────────────────────────────────────────────
//
// These tests run *in the same process* as the binary, so `cargo
// llvm-cov` (and Codecov) see line coverage. The integration tests
// in tests/cli_smoke.rs spawn the built binary as a subprocess —
// great for end-to-end smoke, useless for instrumented coverage.
// Both layers are kept.

#[cfg(test)]
mod tests {
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
            let out =
                dispatch(&s(&[arg]), &mut Cursor::new(Vec::new()));
            assert_eq!(out.exit, 0);
            assert!(out.stdout.contains("USAGE"));
            assert!(out.stderr.is_empty());
        }
    }

    #[test]
    fn version_long_and_short() {
        for arg in ["--version", "-V"] {
            let out =
                dispatch(&s(&[arg]), &mut Cursor::new(Vec::new()));
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
            &s(&[
                "render",
                path.to_str().unwrap(),
                "--set",
                "title=Hi",
            ]),
            &mut Cursor::new(Vec::new()),
        );
        assert_eq!(out.exit, 0);
        assert_eq!(out.stdout, "<h1>Hi</h1>");
    }

    #[test]
    fn render_missing_template_arg_errors() {
        let out =
            dispatch(&s(&["render"]), &mut Cursor::new(Vec::new()));
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
}
