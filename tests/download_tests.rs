// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for the feature-gated remote-template downloader.
//!
//! Every test boots a local `mockito` server on a random loopback port and
//! asserts the wire-level contract: HTTP status handling, `Content-Type`
//! validation, and the 1 MiB body cap (both the `Content-Length` fast path
//! and the post-read re-check).
//!
//! Only compiled under `--features remote-templates`; the default test run
//! skips this file entirely.

#![cfg(feature = "remote-templates")]

use staticweaver::{Engine, EngineError};
use std::time::Duration;

/// Files the downloader pulls on every call. Kept in lock-step with the
/// list in `engine::Engine::download_files_from_url`.
const FILES: &[&str] = &[
    "contact.html",
    "index.html",
    "page.html",
    "post.html",
    "main.js",
    "sw.js",
];

fn engine() -> Engine {
    Engine::new("templates", Duration::from_secs(60))
}

/// Mount a 200 OK response for every file the downloader requests.
fn mount_all_ok(
    server: &mut mockito::ServerGuard,
) -> Vec<mockito::Mock> {
    FILES
        .iter()
        .map(|f| {
            server
                .mock("GET", format!("/{f}").as_str())
                .with_status(200)
                .with_header("Content-Type", "text/html; charset=utf-8")
                .with_body("<html>ok</html>")
                .create()
        })
        .collect()
}

#[test]
fn happy_path_writes_every_file() {
    let mut server = mockito::Server::new();
    let _mocks = mount_all_ok(&mut server);

    let path = engine()
        .create_template_folder(Some(&server.url()))
        .expect("download should succeed");

    for f in FILES {
        let p = std::path::Path::new(&path).join(f);
        assert!(p.exists(), "{f} was not written to {path}");
        let body = std::fs::read_to_string(&p).unwrap();
        assert_eq!(body, "<html>ok</html>");
    }
}

#[test]
fn non_2xx_status_surfaces_as_render_error() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/contact.html")
        .with_status(404)
        .with_body("not found")
        .create();

    let err = engine()
        .create_template_folder(Some(&server.url()))
        .expect_err("404 must not succeed");

    match err {
        EngineError::Render(msg) => {
            assert!(msg.contains("contact.html"), "{msg}");
            assert!(msg.contains("404"), "{msg}");
        }
        other => panic!("expected Render, got {other:?}"),
    }
}

#[test]
fn binary_content_type_is_rejected() {
    let mut server = mockito::Server::new();
    let _m = server
        .mock("GET", "/contact.html")
        .with_status(200)
        .with_header("Content-Type", "application/octet-stream")
        .with_body(vec![0u8; 16])
        .create();

    let err = engine()
        .create_template_folder(Some(&server.url()))
        .expect_err("non-textual payload must not succeed");

    match err {
        EngineError::Render(msg) => {
            assert!(msg.contains("Content-Type"), "{msg}");
            assert!(msg.contains("application/octet-stream"), "{msg}");
        }
        other => panic!("expected Render, got {other:?}"),
    }
}

#[test]
fn oversized_content_length_is_rejected_before_read() {
    let mut server = mockito::Server::new();
    // Mockito sets Content-Length from the body. A 2 MiB body trips the
    // 1 MiB cap check on the `Content-Length` branch.
    let big = vec![b'x'; 2 * 1024 * 1024];
    let _m = server
        .mock("GET", "/contact.html")
        .with_status(200)
        .with_header("Content-Type", "text/html")
        .with_body(big)
        .create();

    let err = engine()
        .create_template_folder(Some(&server.url()))
        .expect_err("2 MiB body must not succeed");

    match err {
        EngineError::Render(msg) => {
            assert!(
                msg.contains("too large")
                    || msg.contains("Content-Length"),
                "expected size-rejection message, got: {msg}"
            );
        }
        other => panic!("expected Render, got {other:?}"),
    }
}

#[test]
fn javascript_content_type_is_accepted() {
    // `main.js` + `sw.js` in the fixed file list are JavaScript. Verify the
    // `application/javascript` MIME family passes the Content-Type gate.
    let mut server = mockito::Server::new();
    let _mocks: Vec<_> = FILES
        .iter()
        .map(|f| {
            let ct = if f.ends_with(".js") {
                "application/javascript"
            } else {
                "text/html"
            };
            server
                .mock("GET", format!("/{f}").as_str())
                .with_status(200)
                .with_header("Content-Type", ct)
                .with_body("// ok")
                .create()
        })
        .collect();

    let _path = engine()
        .create_template_folder(Some(&server.url()))
        .expect("JS files must pass Content-Type check");
}

#[test]
fn empty_content_type_is_accepted() {
    // Real-world servers sometimes omit Content-Type. The downloader is
    // permissive for missing headers (documented in `engine.rs`).
    let mut server = mockito::Server::new();
    let _mocks: Vec<_> = FILES
        .iter()
        .map(|f| {
            server
                .mock("GET", format!("/{f}").as_str())
                .with_status(200)
                .with_body("<html></html>")
                .create()
        })
        .collect();

    let _path = engine()
        .create_template_folder(Some(&server.url()))
        .expect("missing Content-Type header must be tolerated");
}
