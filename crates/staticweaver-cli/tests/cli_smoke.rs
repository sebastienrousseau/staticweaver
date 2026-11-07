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

/// `render --watch` smoke tests (issue #53). Unix only: they stop the
/// process with SIGINT through `kill -INT`, which Windows lacks.
#[cfg(unix)]
mod watch {
    use super::bin;
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Child, Stdio};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::{Duration, Instant};

    /// Generous: a slow CI runner must not turn latency into failure.
    const GENEROUS: Duration = Duration::from_secs(20);

    fn lines(stream: impl Read + Send + 'static) -> Receiver<String> {
        let (tx, rx) = mpsc::channel();
        let _ = thread::spawn(move || {
            for line in
                BufReader::new(stream).lines().map_while(Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        rx
    }

    /// Waits for a line containing `needle`, returning every line seen.
    fn wait_for(rx: &Receiver<String>, needle: &str) -> Vec<String> {
        let deadline = Instant::now() + GENEROUS;
        let mut seen = Vec::new();
        while let Some(left) =
            deadline.checked_duration_since(Instant::now())
        {
            match rx.recv_timeout(left) {
                Ok(line) => {
                    let hit = line.contains(needle);
                    seen.push(line);
                    if hit {
                        return seen;
                    }
                }
                Err(_) => break,
            }
        }
        panic!("no line containing {needle:?} within {GENEROUS:?}; saw {seen:?}");
    }

    fn sigint_and_wait(child: &mut Child) -> std::process::ExitStatus {
        let status = std::process::Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success(), "kill -INT failed");
        let deadline = Instant::now() + GENEROUS;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "did not exit after SIGINT"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn spawn_watch(file: &std::path::Path) -> Child {
        bin()
            .args([
                "render",
                file.to_str().unwrap(),
                "--set",
                "name=Ada",
                "--watch",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    #[test]
    fn cli_watch_starts_and_exits_cleanly_on_sigint() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "Hi {{name}}!").unwrap();
        let mut child = spawn_watch(&file);
        let out = lines(child.stdout.take().unwrap());
        let err = lines(child.stderr.take().unwrap());

        let _ = wait_for(&out, "Hi Ada!");
        // The handler is installed before this line is printed.
        let _ = wait_for(&err, "watching");

        let status = sigint_and_wait(&mut child);
        assert!(status.success(), "exit status {status:?}");
        let _ = wait_for(&err, "stopped watching");
    }

    /// OS-DEPENDENT: relies on the platform file watcher noticing the
    /// edit (inotify on Linux, FSEvents on macOS).
    #[test]
    fn cli_watch_rerenders_after_an_edit_os_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, "Hi {{name}}!").unwrap();
        let mut child = spawn_watch(&file);
        let out = lines(child.stdout.take().unwrap());
        let err = lines(child.stderr.take().unwrap());
        let _ = wait_for(&out, "Hi Ada!");
        let _ = wait_for(&err, "watching");

        std::fs::write(&file, "Bye {{name}}!").unwrap();
        let _ = wait_for(&out, "Bye Ada!");

        assert!(sigint_and_wait(&mut child).success());
    }
}
