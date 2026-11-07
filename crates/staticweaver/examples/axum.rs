// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal Axum integration example.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example axum --features axum-example
//! ```
//!
//! Then open <http://127.0.0.1:3030/>.
//!
//! Demonstrates two patterns:
//!
//! 1. **Render to a `String`** (`/`) — the simplest case. Fits any
//!    framework that wants `impl IntoResponse`.
//! 2. **Render to a `Vec<u8>`** via `Engine::render_to` (`/stream`)
//!    — saves the `String -> bytes` conversion the framework would
//!    otherwise do internally. Same shape works against any
//!    `std::io::Write` sink (a `File`, an `actix_web::HttpResponse`
//!    body builder, a `hyper::Body` channel, …).

use axum::{
    extract::Path as AxumPath,
    http::header::CONTENT_TYPE,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use staticweaver::{Context, Engine};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

/// Shared engine instance — built once at startup, cloned cheaply
/// into request handlers via `Arc`. The engine is `Send + Sync`,
/// so a single instance can serve every request concurrently.
type SharedEngine = Arc<Engine>;

#[tokio::main]
async fn main() {
    let mut engine = Engine::new("", Duration::from_secs(60));
    // A custom filter — same pattern works for tests, partials, etc.
    let _ = engine.add_filter(
        "shout",
        Arc::new(|input, _args| {
            Ok(format!("{}!!!", input.to_uppercase()))
        }),
    );
    let engine: SharedEngine = Arc::new(engine);

    let app = Router::new()
        .route(
            "/",
            get({
                let engine = engine.clone();
                move || home_handler(engine)
            }),
        )
        .route(
            "/stream",
            get({
                let engine = engine.clone();
                move || stream_handler(engine)
            }),
        )
        .route(
            "/hello/{name}",
            get({
                let engine = engine.clone();
                move |path: AxumPath<String>| {
                    hello_handler(engine, path)
                }
            }),
        );

    let listener = TcpListener::bind("127.0.0.1:3030").await.unwrap();
    println!("staticweaver + axum listening on http://127.0.0.1:3030/");
    axum::serve(listener, app).await.unwrap();
}

/// Pattern 1: render to `String`, return as `Html<String>`.
async fn home_handler(engine: SharedEngine) -> Html<String> {
    let mut ctx = Context::new();
    ctx.set("title".to_string(), "staticweaver + axum".to_string());
    ctx.set_value(
        "links".to_string(),
        vec!["/", "/stream", "/hello/Ada"],
    );
    let template = "\
<!doctype html><html><body>
<h1>{{ title | shout }}</h1>
<ul>
{{#each links}}<li><a href=\"{{this}}\">{{this}}</a></li>
{{/each}}
</ul>
</body></html>";
    Html(engine.render_template(template, &ctx).unwrap())
}

/// Pattern 2: render directly to a `Vec<u8>` via `render_to`,
/// then hand the bytes to the response. Saves the implicit
/// `String -> Vec<u8>` step in the framework's IntoResponse path.
async fn stream_handler(engine: SharedEngine) -> Response {
    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());
    let template = "Hello, {{ name }}! (rendered via render_to)";

    let mut buf: Vec<u8> = Vec::new();
    engine.render_to(template, &ctx, &mut buf).expect("render");
    ([(CONTENT_TYPE, "text/plain; charset=utf-8")], buf).into_response()
}

/// Pattern 3: per-request context from a path parameter.
async fn hello_handler(
    engine: SharedEngine,
    AxumPath(name): AxumPath<String>,
) -> Html<String> {
    let mut ctx = Context::new();
    ctx.set("name".to_string(), name);
    Html(
        engine
            .render_template("<h1>Hello, {{ name }}!</h1>", &ctx)
            .unwrap(),
    )
}
