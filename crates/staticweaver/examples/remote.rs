// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024-2026 StaticWeaver. All rights reserved.

//! Remote template fetching against a local mock HTTP server.
//!
//! Demonstrates `create_template_folder(Some(url))` under the opt-in
//! `remote-templates` cargo feature. A `mockito` server stands in for
//! `github.com` so the example runs offline.
//!
//! Run: `cargo run --example remote --features remote-templates`
//!
//! Without the feature flag the example compiles to a single `println!`
//! that explains how to enable it.

#[path = "support.rs"]
mod support;

#[cfg(feature = "remote-templates")]
fn main() {
    use staticweaver::Engine;
    use std::time::Duration;

    support::header("staticweaver -- remote");

    let mut server = mockito::Server::new();
    let files = [
        "contact.html",
        "index.html",
        "page.html",
        "post.html",
        "main.js",
        "sw.js",
    ];
    let _mocks: Vec<_> = files
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
                .with_body(format!("<!-- {f} -->"))
                .create()
        })
        .collect();

    let engine = support::task("Build an Engine", || {
        Engine::new("templates", Duration::from_secs(60))
    });

    let path = support::task_result(
        "Fetch the template set from the mock server",
        || engine.create_template_folder(Some(&server.url())),
    )
    .expect("mock server should serve every file");

    support::task_with_output(
        "Inspect the downloaded directory",
        || {
            let mut out = vec![format!("target = {path}")];
            for f in &files {
                let exists =
                    std::path::Path::new(&path).join(f).exists();
                out.push(format!("{f} present: {exists}"));
            }
            out
        },
    );

    support::summary(3);
}

#[cfg(not(feature = "remote-templates"))]
fn main() {
    support::header("staticweaver -- remote");
    println!(
        "  This example requires the `remote-templates` cargo feature."
    );
    println!(
        "  Run:  cargo run --example remote --features remote-templates"
    );
    println!();
}
