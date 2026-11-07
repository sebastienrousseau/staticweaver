// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # WASI Preview 2 example (issue #50)
//!
//! Renders a page with `staticweaver-core` inside a WebAssembly
//! component. Templates come from an in-memory map, so the component
//! needs no filesystem or network capability: only stdout.
//!
//! Build and run (Wasmtime 48.0.2, the version this example was run
//! with; any Wasmtime with WASI 0.2 support should work):
//!
//! ```text
//! rustup target add wasm32-wasip2
//! cargo build -p staticweaver-core --example wasi --target wasm32-wasip2 --release
//! wasmtime run target/wasm32-wasip2/release/examples/wasi.wasm
//! ```
//!
//! It also runs natively: `cargo run -p staticweaver-core --example wasi`.

use staticweaver_core::{Context, EngineError, Renderer, Value};
use std::collections::BTreeMap;

fn main() -> Result<(), EngineError> {
    let mut templates = BTreeMap::new();
    let _ = templates.insert(
        "base".to_string(),
        "<h1>{{#block \"title\"}}Untitled{{/block}}</h1>\n\
         {{#block \"body\"}}{{/block}}"
            .to_string(),
    );
    let _ = templates.insert(
        "page".to_string(),
        "{{#extends \"base\"}}\
         {{#block \"title\"}}{{ title | uppercase }}{{/block}}\
         {{#block \"body\"}}<ul>{{#each items}}<li>{{this}}</li>{{/each}}</ul>\n{{/block}}"
            .to_string(),
    );

    let mut ctx = Context::new();
    ctx.set("title".to_string(), "Hello from WASI <0.2>".to_string());
    ctx.set_value(
        "items".to_string(),
        vec![
            Value::from("no_std core"),
            Value::from("in-memory loader"),
        ],
    );

    let html = Renderer::new(&templates).render_named("page", &ctx)?;
    print!("{html}");
    Ok(())
}
