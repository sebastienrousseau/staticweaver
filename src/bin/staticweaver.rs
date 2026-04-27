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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{HELP}");
        return ExitCode::from(2);
    }
    if matches!(args[0].as_str(), "--help" | "-h" | "help") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if matches!(args[0].as_str(), "--version" | "-V") {
        println!("staticweaver {VERSION}");
        return ExitCode::SUCCESS;
    }
    if args[0] != "render" {
        eprintln!("error: unknown subcommand `{}`\n", args[0]);
        eprintln!("{HELP}");
        return ExitCode::from(2);
    }
    match cmd_render(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_render(args: &[String]) -> Result<(), String> {
    let mut template_arg: Option<&str> = None;
    let mut sets: Vec<(String, String)> = Vec::new();
    let mut no_escape = false;

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
                sets.push((k.to_string(), v.to_string()));
                i += 2;
            }
            "--no-escape" => {
                no_escape = true;
                i += 1;
            }
            // `-` alone means "read template from stdin" — not an
            // option flag. Anything else starting with `-` is.
            other
                if (other == "-" || !other.starts_with('-'))
                    && template_arg.is_none() =>
            {
                template_arg = Some(other);
                i += 1;
            }
            other => {
                return Err(format!("unknown arg `{other}`"));
            }
        }
    }

    let tmpl_src = template_arg
        .ok_or_else(|| "missing <template> argument".to_string())?;
    let template = if tmpl_src == "-" {
        let mut s = String::new();
        let _ = std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| format!("reading stdin: {e}"))?;
        s
    } else {
        std::fs::read_to_string(tmpl_src).map_err(|e| {
            format!("reading template `{tmpl_src}`: {e}")
        })?
    };

    let mut ctx = Context::new();
    for (k, v) in sets {
        ctx.set(k, v);
    }

    let engine = Engine::new("", Duration::from_secs(60))
        .with_html_escape(!no_escape);
    let out = engine
        .render_template(&template, &ctx)
        .map_err(|e| format!("render: {e}"))?;
    print!("{out}");
    Ok(())
}
