// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `staticweaver render <file> --watch` (issue #53).
//!
//! Renders once, then re-renders whenever the template file or
//! anything under its directory changes, using
//! `staticweaver::watch::WatchLoader` as the engine's loader so
//! partials are cached between renders and dropped on change. Ctrl-C
//! (SIGINT) stops the loop and the process exits 0.

use crate::{context_from, RenderArgs};
use staticweaver::engine::FsLoader;
use staticweaver::watch::WatchLoader;
use staticweaver::{Context, Engine};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

/// How often the loop checks for Ctrl-C between change batches.
const POLL: Duration = Duration::from_millis(100);

/// Installs the Ctrl-C handler and runs the watch loop.
pub(crate) fn run<O: Write, E: Write>(
    parsed: &RenderArgs,
    stdout: &mut O,
    stderr: &mut E,
) -> u8 {
    let stop = match ctrl_c() {
        Ok(stop) => stop,
        Err(msg) => return fail(stderr, &msg),
    };
    serve(parsed, &stop, stdout, stderr)
}

/// A receiver that gets `()` on each Ctrl-C (SIGINT on Unix).
fn ctrl_c() -> Result<Receiver<()>, String> {
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .map_err(|e| format!("installing the Ctrl-C handler: {e}"))?;
    Ok(rx)
}

fn fail<E: Write>(stderr: &mut E, msg: &str) -> u8 {
    let _ = writeln!(stderr, "error: {msg}");
    1
}

/// Renders, then re-renders on every change batch until `stop`
/// receives a value or hangs up. Render errors are reported and the
/// loop keeps watching, so a half-saved template does not end it.
pub(crate) fn serve<O: Write, E: Write>(
    parsed: &RenderArgs,
    stop: &Receiver<()>,
    stdout: &mut O,
    stderr: &mut E,
) -> u8 {
    let path = match template_path(parsed) {
        Ok(path) => path,
        Err(msg) => return fail(stderr, &msg),
    };
    let dir = watch_root(path);
    let loader =
        match WatchLoader::new(FsLoader::new(PathBuf::new()), &dir) {
            Ok(loader) => Arc::new(loader),
            Err(e) => {
                return fail(
                    stderr,
                    &format!("watching {}: {e}", dir.display()),
                )
            }
        };
    let changes = loader.subscribe();
    let engine = Engine::with_loader(loader, Duration::from_secs(60))
        .with_html_escape(!parsed.no_escape);
    let ctx = context_from(&parsed.sets);

    let job = Job {
        engine: &engine,
        path,
        ctx: &ctx,
    };
    job.render(stdout, stderr);
    let _ =
        writeln!(stderr, "watching {} (Ctrl-C to stop)", dir.display());
    watch_loop(&job, &changes, stop, stdout, stderr);
    let _ = writeln!(stderr, "stopped watching");
    0
}

/// What one render needs: the engine, the template file, the context.
struct Job<'a> {
    engine: &'a Engine,
    path: &'a Path,
    ctx: &'a Context,
}

impl Job<'_> {
    fn render<O: Write, E: Write>(
        &self,
        stdout: &mut O,
        stderr: &mut E,
    ) {
        render_once(self.engine, self.path, self.ctx, stdout, stderr);
    }
}

/// Re-renders on each change batch (clearing the render cache, whose
/// pages embed partials) until `stop` fires or the change source
/// hangs up.
fn watch_loop<O: Write, E: Write>(
    job: &Job<'_>,
    changes: &Receiver<Vec<PathBuf>>,
    stop: &Receiver<()>,
    stdout: &mut O,
    stderr: &mut E,
) {
    while !stopped(stop) {
        match changes.recv_timeout(POLL) {
            Ok(_) => {
                job.engine.clear_cache();
                job.render(stdout, stderr);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// The template file; `--watch` cannot follow stdin.
fn template_path(parsed: &RenderArgs) -> Result<&Path, String> {
    match parsed.template_arg.as_deref() {
        None => Err("missing <template> argument".to_string()),
        Some("-") => {
            Err("--watch needs a template file, not `-` (stdin)"
                .to_string())
        }
        Some(file) => Ok(Path::new(file)),
    }
}

/// The directory to watch: the template's own directory.
fn watch_root(path: &Path) -> PathBuf {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn stopped(stop: &Receiver<()>) -> bool {
    !matches!(stop.try_recv(), Err(TryRecvError::Empty))
}

fn render_once<O: Write, E: Write>(
    engine: &Engine,
    path: &Path,
    ctx: &Context,
    stdout: &mut O,
    stderr: &mut E,
) {
    let rendered = std::fs::read_to_string(path)
        .map_err(|e| {
            format!("reading template `{}`: {e}", path.display())
        })
        .and_then(|body| {
            engine
                .render_template(&body, ctx)
                .map_err(|e| format!("render: {e}"))
        });
    match rendered {
        Ok(out) => {
            let _ = writeln!(stdout, "{out}");
            let _ = stdout.flush();
        }
        Err(msg) => {
            let _ = fail(stderr, &msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(template: Option<&str>) -> RenderArgs {
        RenderArgs {
            template_arg: template.map(str::to_string),
            sets: vec![("name".to_string(), "Ada".to_string())],
            no_escape: false,
            watch: true,
        }
    }

    /// A stop signal that is already pending: the loop renders once,
    /// then exits cleanly without waiting for any file change.
    fn stopped_now() -> Receiver<()> {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        rx
    }

    fn serve_strings(parsed: &RenderArgs) -> (u8, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = serve(parsed, &stopped_now(), &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn renders_once_then_stops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "Hi {{name}}!").unwrap();
        let (code, out, err) = serve_strings(&args(file.to_str()));
        assert_eq!(code, 0, "{err}");
        assert_eq!(out, "Hi Ada!\n");
        assert!(err.contains("watching"), "{err}");
        assert!(err.contains("stopped watching"), "{err}");
    }

    #[test]
    fn a_render_error_is_reported_and_watching_continues() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "{{missing}}").unwrap();
        let (code, out, err) = serve_strings(&args(file.to_str()));
        assert_eq!(code, 0);
        assert!(out.is_empty());
        assert!(err.contains("error: render"), "{err}");
        assert!(err.contains("stopped watching"), "{err}");
    }

    #[test]
    fn stdin_and_missing_templates_are_refused() {
        let (code, _, err) = serve_strings(&args(Some("-")));
        assert_eq!(code, 1);
        assert!(err.contains("not `-`"), "{err}");
        let (code, _, err) = serve_strings(&args(None));
        assert_eq!(code, 1);
        assert!(err.contains("missing <template>"), "{err}");
    }

    #[test]
    fn an_unwatchable_directory_is_an_error() {
        let (code, _, err) = serve_strings(&args(Some(
            "/no/such/staticweaver/dir/page.html",
        )));
        assert_eq!(code, 1);
        assert!(err.contains("watching /no/such"), "{err}");
    }

    #[test]
    fn a_bare_file_name_watches_the_current_directory() {
        assert_eq!(
            watch_root(Path::new("page.html")),
            PathBuf::from(".")
        );
        assert_eq!(
            watch_root(Path::new("a/page.html")),
            PathBuf::from("a")
        );
    }

    /// Drives the loop with synthetic change batches (no OS watcher):
    /// a batch re-renders the current file contents, a quiet period
    /// times out and keeps waiting, and a hung-up source ends it.
    #[test]
    fn the_loop_rerenders_on_each_batch_until_the_source_hangs_up() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "v1 {{name}}").unwrap();
        let engine = Engine::new("", Duration::from_secs(60));
        let ctx =
            context_from(&[("name".to_string(), "Ada".to_string())]);
        let job = Job {
            engine: &engine,
            path: &file,
            ctx: &ctx,
        };
        let (tx, changes) = mpsc::channel();
        let (_stop_tx, stop) = mpsc::channel::<()>();
        std::fs::write(&file, "v2 {{name}}").unwrap();
        tx.send(vec![file.clone()]).unwrap();
        let sender = std::thread::spawn(move || {
            // Longer than POLL, so the loop also takes the timeout arm.
            std::thread::sleep(POLL * 3);
            drop(tx);
        });
        let (mut out, mut err) = (Vec::new(), Vec::new());
        watch_loop(&job, &changes, &stop, &mut out, &mut err);
        sender.join().unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "v2 Ada\n");
        assert!(err.is_empty());
    }

    /// `run` installs the process-wide Ctrl-C handler; ctrlc allows one
    /// per process, so this is the only test that calls it. The first
    /// call installs it and fails on the stdin template; the second
    /// reports that the handler cannot be installed again.
    #[test]
    fn run_installs_the_handler_once() {
        // First through the binary's own entry point, so `render
        // --watch` is routed here.
        let argv: Vec<String> = ["render", "-", "--watch"]
            .iter()
            .map(|a| (*a).to_string())
            .collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code =
            crate::run(argv, &mut std::io::empty(), &mut out, &mut err);
        assert_eq!(code, 1);
        assert!(String::from_utf8_lossy(&err).contains("not `-`"));
        let mut err = Vec::new();
        assert_eq!(run(&args(Some("-")), &mut out, &mut err), 1);
        let err = String::from_utf8(err).unwrap();
        assert!(err.contains("installing the Ctrl-C handler"), "{err}");
    }

    #[test]
    fn a_hung_up_stop_channel_also_stops() {
        let (tx, rx) = mpsc::channel::<()>();
        drop(tx);
        assert!(stopped(&rx));
        let (_tx, rx) = mpsc::channel::<()>();
        assert!(!stopped(&rx));
    }
}
