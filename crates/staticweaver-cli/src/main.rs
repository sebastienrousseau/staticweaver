// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `staticweaver` CLI — render templates from the shell.
//!
//! ```text
//! USAGE
//!   staticweaver render <template> [--set key=value ...] [--no-escape] [--watch]
//!   staticweaver --help
//!   staticweaver --version
//!
//! ARGS
//!   <template>          Template file path, or `-` to read from stdin.
//!
//! OPTIONS
//!   --set KEY=VALUE     Set a context binding. Repeatable.
//!   --no-escape         Disable HTML escape.
//!   --watch             Re-render whenever the template or anything
//!                       under its directory changes. Ctrl-C stops.
//!   --help, -h          Print this message.
//!   --version, -V       Print the binary version.
//!
//! EXAMPLES
//!   staticweaver render hello.html --set name=Ada
//!   echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
//!   staticweaver render page.html --set name=Ada --watch
//! ```
//!
//! Hand-rolled arg parsing — no `clap` dep, so the binary stays
//! lightweight and `cargo install staticweaver` produces a small
//! executable.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

mod watch;

use staticweaver::{Context, Engine};
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
USAGE
  staticweaver render <template> [--set key=value ...] [--no-escape] [--watch]
  staticweaver --help
  staticweaver --version

ARGS
  <template>          Template file path, or `-` to read from stdin.

OPTIONS
  --set KEY=VALUE     Set a context binding. Repeatable.
  --no-escape         Disable HTML escape.
  --watch             Re-render whenever the template or anything
                      under its directory changes. Ctrl-C stops.
  --help, -h          Print this message.
  --version, -V       Print the binary version.

EXAMPLES
  staticweaver render hello.html --set name=Ada
  echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
  staticweaver render page.html --set name=Ada --watch
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
    watch: bool,
}

fn main() -> ExitCode {
    let exit = run(
        std::env::args().skip(1).collect(),
        &mut std::io::stdin(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    ExitCode::from(exit)
}

/// Process-runner core. Pulled out of `main` so the same logic can
/// be exercised in-process by unit tests that pass synthetic
/// readers and writers — coverage tools (notably `cargo tarpaulin`
/// in CI) don't instrument the binary launched by subprocess
/// integration tests, so this in-process path is what closes the
/// coverage loop on the CLI's terminal-write branches.
///
/// Returns the exit code as `u8` (not `std::process::ExitCode`)
/// so unit tests can compare against an integer literal —
/// `ExitCode`'s `Debug` format is platform-specific (Windows
/// shows it differently from Unix), and the type doesn't expose
/// the underlying byte.
fn run<R: Read, O: std::io::Write, E: std::io::Write>(
    args: Vec<String>,
    stdin: &mut R,
    stdout: &mut O,
    stderr: &mut E,
) -> u8 {
    if let Some(parsed) = watch_request(&args) {
        return watch::run(&parsed, stdout, stderr);
    }
    let out = dispatch(&args, stdin);
    if !out.stdout.is_empty() {
        let _ = stdout.write_all(out.stdout.as_bytes());
    }
    if !out.stderr.is_empty() {
        let _ = stderr.write_all(out.stderr.as_bytes());
    }
    out.exit
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

/// `render ... --watch` with otherwise valid arguments. Anything else
/// (including a malformed `--watch` command line) goes through
/// `dispatch`, which reports the error.
fn watch_request(args: &[String]) -> Option<RenderArgs> {
    if args.first().map(String::as_str) != Some("render") {
        return None;
    }
    parse_render_args(&args[1..])
        .ok()
        .filter(|parsed| parsed.watch)
}

fn parse_render_args(args: &[String]) -> Result<RenderArgs, String> {
    let mut out = RenderArgs::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--set" => out.sets.push(parse_set(rest.next())?),
            "--no-escape" => out.no_escape = true,
            "--watch" => out.watch = true,
            other => out.take_template(other)?,
        }
    }
    Ok(out)
}

/// The `KEY=VALUE` operand of `--set`.
fn parse_set(arg: Option<&String>) -> Result<(String, String), String> {
    let pair = arg.ok_or_else(|| {
        "--set requires a KEY=VALUE argument".to_string()
    })?;
    let (k, v) = pair.split_once('=').ok_or_else(|| {
        format!("--set expected KEY=VALUE, got `{pair}`")
    })?;
    Ok((k.to_string(), v.to_string()))
}

impl RenderArgs {
    /// Takes `arg` as the template operand. `-` alone means "read the
    /// template from stdin", not an option flag; anything else
    /// starting with `-`, or a second operand, is an unknown arg.
    fn take_template(&mut self, arg: &str) -> Result<(), String> {
        if (arg == "-" || !arg.starts_with('-'))
            && self.template_arg.is_none()
        {
            self.template_arg = Some(arg.to_string());
            Ok(())
        } else {
            Err(format!("unknown arg `{arg}`"))
        }
    }
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

    let ctx = context_from(&parsed.sets);

    let engine = Engine::new("", Duration::from_secs(60))
        .with_html_escape(!parsed.no_escape);
    engine
        .render_template(&template, &ctx)
        .map_err(|e| format!("render: {e}"))
}

/// The render context built from `--set` pairs.
fn context_from(sets: &[(String, String)]) -> Context {
    let mut ctx = Context::new();
    for (k, v) in sets {
        ctx.set(k.clone(), v.clone());
    }
    ctx
}

// ─── Unit tests ─────────────────────────────────────────────────────
//
// These tests run *in the same process* as the binary, so `cargo
// llvm-cov` (and Codecov) see line coverage. The integration tests
// in tests/cli_smoke.rs spawn the built binary as a subprocess —
// great for end-to-end smoke, useless for instrumented coverage.
// Both layers are kept.

#[cfg(test)]
mod tests;
