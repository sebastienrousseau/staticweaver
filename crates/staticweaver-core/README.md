# staticweaver-core

The core of [staticweaver](https://crates.io/crates/staticweaver): the
template parser, expression evaluator, renderer, idempotent HTML escape,
the `Value` / `Context` data model and the error types.

It builds for `no_std + alloc` targets (embedded, `wasm32`, edge
runtimes) with `default-features = false`. Most users want the
`staticweaver` crate, which re-exports everything here and adds the
`std` pieces: `Engine`, the filesystem loader, the render cache and the
remote-template fetcher.

```rust
use staticweaver_core::{Context, Renderer};
use std::collections::BTreeMap;

let mut templates = BTreeMap::new();
let _ = templates.insert("greet".to_string(), "Hi {{name}}!".to_string());

let mut ctx = Context::new();
ctx.set("name".to_string(), "Ada".to_string());

let out = Renderer::new(&templates).render_named("greet", &ctx).unwrap();
assert_eq!(out, "Hi Ada!");
```

## Features

| Feature | Default | Effect |
| :--- | :--- | :--- |
| `std` | yes | `Value::Map` is `FnvHashMap`, `EngineError::Io` exists, names are checked with `euxis_commons::path::safe_join`. Without it, `Map` is `BTreeMap`. |
| `json` | no | The `{{ value \| json }}` filter. |
| `remote-templates` | no | Adds the `EngineError::Reqwest` variant used by the facade. |

Licensed under MIT or Apache-2.0, at your option.
