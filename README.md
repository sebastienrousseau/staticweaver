<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<p align="center">
  <img src="https://cloudcdn.pro/staticweaver/v1/logos/staticweaver.svg" alt="StaticWeaver logo" width="128" />
</p>

<h1 align="center">StaticWeaver</h1>

<p align="center">
  <strong>Small, safe, cacheable. A templating engine for Rust that escapes HTML by default.</strong>
</p>

<p align="center">
  <a href="https://github.com/sebastienrousseau/staticweaver/actions"><img src="https://img.shields.io/github/actions/workflow/status/sebastienrousseau/staticweaver/ci.yml?style=for-the-badge&logo=github" alt="Build" /></a>
  <a href="https://crates.io/crates/staticweaver"><img src="https://img.shields.io/crates/v/staticweaver.svg?style=for-the-badge&color=fc8d62&logo=rust" alt="Crates.io" /></a>
  <a href="https://docs.rs/staticweaver"><img src="https://img.shields.io/badge/docs.rs-staticweaver-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" alt="Docs.rs" /></a>
  <a href="https://codecov.io/gh/sebastienrousseau/staticweaver"><img src="https://img.shields.io/codecov/c/github/sebastienrousseau/staticweaver?style=for-the-badge&logo=codecov" alt="Coverage" /></a>
  <a href="https://lib.rs/crates/staticweaver"><img src="https://img.shields.io/badge/lib.rs-v0.0.2-orange.svg?style=for-the-badge" alt="lib.rs" /></a>
</p>

---

## Contents

- [Install](#install) -- Cargo, source, MSRV
- [Quick Start](#quick-start) -- render a template in 10 lines
- [Overview](#overview) -- what staticweaver does
- [Features](#features) -- capability matrix
- [Library Usage](#library-usage) -- rendering, escaping, delimiters, caching, remote templates
- [Configuration](#configuration) -- engine and cache options
- [Examples](#examples) -- five branded examples
- [Development](#development) -- make targets, CI
- [Security](#security) -- safety guarantees
- [License](#license)

---

## Install

```toml
[dependencies]
staticweaver = "0.0.2"
```

### Optional: remote templates

Fetching templates from an HTTP/S URL is gated behind the `remote-templates` feature. The default build has no networking code.

```toml
[dependencies]
staticweaver = { version = "0.0.2", features = ["remote-templates"] }
```

### Build from source

```bash
git clone https://github.com/sebastienrousseau/staticweaver.git
cd staticweaver
make          # check + clippy + test
```

Requires **Rust 1.68.0+**. Tested on Linux, macOS, and Windows.

---

## Quick Start

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));

let mut ctx = Context::new();
ctx.set("title".to_string(), "My Page".to_string());
ctx.set("body".to_string(), "Hello, World!".to_string());

let template = "<h1>{{title}}</h1><p>{{body}}</p>";
let output = engine.render_template(template, &ctx).unwrap();
assert_eq!(output, "<h1>My Page</h1><p>Hello, World!</p>");
```

Use [`Engine::render_page`](https://docs.rs/staticweaver) to render a `.html` file from the template directory instead of a literal string.

---

## Overview

StaticWeaver is a small templating engine that substitutes `{{name}}` tags against a `Context` and writes the result back as a `String`. It is designed to be **safe by default, cacheable, and dependency-light**: most templating crates pick one of those — staticweaver picks all three.

- **HTML-escaped by default** -- `&<>"'` in context values become entities; `{{!key}}` opts out per tag
- **Path-validated `render_page`** -- rejects `..`, `/`, `\`, null bytes in layout names
- **`#![forbid(unsafe_code)]`** -- enforced at the crate root
- **TTL cache** -- generic `Cache<K, V>` with optional capacity bound
- **Custom delimiters** -- swap `{{ }}` for any pair at runtime
- **Opt-in networking** -- remote template fetch lives behind the `remote-templates` cargo feature
- **Bounded HTTP** -- remote fetches cap bodies at 1 MiB
- **Pure Rust** -- no C bindings, no FFI, no build script
- **MSRV 1.68** -- stable Rust only

---

## Features

| | |
| :--- | :--- |
| **Rendering** | `render_template(&str, &Context)` for in-memory strings, `render_page(&Context, layout)` for a `.html` file inside `template_path`. Both return `Result<String, EngineError>`. |
| **Context** | `Context::set/get/update/remove`, `with_capacity`, `iter`, `clear`, and a stable `hash()` for cache-key construction. Implements `Deref<Target = FnvHashMap<String, String>>` for power users. |
| **HTML escape** | On by default. Five entities replaced (`& < > " '`). Per-tag opt-out: `{{!body}}`. Global opt-out: `Engine::new(...).with_html_escape(false)`. |
| **Delimiters** | `set_delimiters(open, close)` swaps `{{` / `}}` for any pair. Whitespace around keys is trimmed (`{{ name }}` == `{{name}}`). |
| **Cache** | Generic `Cache<K, V>` with time-based expiration and an optional hard capacity. `insert`, `get`, `ttl`, `refresh`, `update`, `remove`, `contains_key`, `remove_expired`, `clear`, `iter`, `IntoIterator`. |
| **Remote templates** | `create_template_folder(Some(url))` under `--features remote-templates`. 10 s timeout, 1 MiB body cap, status-code check, `rustls-tls-native-roots`. The default URL fallback has been removed. |
| **Errors** | `EngineError` (`Io`, `Render`, `InvalidTemplate`, `Template`, `ResourceNotFound`, `Timeout`, and `Reqwest` under the feature). `TemplateError` with `#[from]` conversions for `io::Error`, `reqwest::Error`, and boxed `EngineError`. |
| **`cargo deny`** | `advisories`, `bans`, `licenses`, `sources` all pass. Yanked crates denied. |

---

## Library Usage

<details>
<summary><b>Basic rendering</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("greeting".to_string(), "Hello".to_string());
ctx.set("name".to_string(), "Alice".to_string());

let out = engine.render_template("{{greeting}}, {{name}}!", &ctx).unwrap();
assert_eq!(out, "Hello, Alice!");
```

Whitespace inside a tag is trimmed, so `{{ name }}` and `{{name}}` are equivalent. A missing key yields `EngineError::Render`; an unclosed or nested tag yields `EngineError::InvalidTemplate`.

</details>

<details>
<summary><b>HTML escaping and the raw opt-out</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("user".to_string(), "<script>alert(1)</script>".to_string());
ctx.set("body".to_string(), "<b>hi</b>".to_string());

// Default: values are escaped.
let safe = engine.render_template("Hi {{user}}", &ctx).unwrap();
assert_eq!(safe, "Hi &lt;script&gt;alert(1)&lt;/script&gt;");

// Per-tag opt-out with `!`.
let raw = engine.render_template("{{!body}}", &ctx).unwrap();
assert_eq!(raw, "<b>hi</b>");

// Global opt-out (non-HTML output, or when you escape upstream yourself).
let plain = Engine::new("", Duration::from_secs(60))
    .with_html_escape(false)
    .render_template("{{body}}", &ctx)
    .unwrap();
assert_eq!(plain, "<b>hi</b>");
```

Only the substituted *values* are escaped — template text itself is emitted verbatim, so `<h1>` in the template stays as `<h1>`.

</details>

<details>
<summary><b>Page rendering (file-backed templates)</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let mut engine = Engine::new("templates", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("title".to_string(), "About".to_string());

// Loads "templates/layout.html", substitutes, caches by (layout, ctx-hash).
let _result = engine.render_page(&ctx, "layout");
```

`render_page` rejects layout names containing `/`, `\`, null bytes, or a `..` segment before touching the filesystem, so `render_page(&ctx, "../../etc/passwd")` returns `EngineError::InvalidTemplate` rather than reading outside the template directory.

</details>

<details>
<summary><b>Custom delimiters</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let mut engine = Engine::new("", Duration::from_secs(60));
engine.set_delimiters("<<", ">>");

let mut ctx = Context::new();
ctx.set("name".to_string(), "Bob".to_string());

let out = engine.render_template("Hello, <<name>>!", &ctx).unwrap();
assert_eq!(out, "Hello, Bob!");
```

Any pair of non-empty strings works. A bare occurrence of either delimiter outside a full `open…close` pair is treated as literal text, not an error.

</details>

<details>
<summary><b>Caching</b></summary>

```rust
use staticweaver::cache::Cache;
use std::time::Duration;

let mut cache: Cache<String, String> =
    Cache::with_capacity(Duration::from_secs(60), 1024);

let _ = cache.insert("greeting".to_string(), "hello".to_string());

assert_eq!(cache.get(&"greeting".to_string()), Some(&"hello".to_string()));
assert!(cache.contains_key(&"greeting".to_string()));

// Periodically drop expired entries.
cache.remove_expired();
```

The `Engine::render_page` path caches by `"{layout}:{ctx.hash()}"`. `Context::hash` sorts keys before hashing so equal contexts always produce equal hashes, making the cache hit deterministically rather than thrashing on insertion order.

Bounded caches silently drop inserts past the cap (new keys only — updating an existing key always succeeds). Call `set_max_cache_size(n)` on the engine to clear the cache if it grows beyond `n` entries.

</details>

<details>
<summary><b>Remote templates (feature-gated)</b></summary>

```toml
[dependencies]
staticweaver = { version = "0.0.2", features = ["remote-templates"] }
```

```rust,ignore
use staticweaver::Engine;
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));
let path = engine
    .create_template_folder(Some("https://example.com/templates/"))?;
println!("downloaded to {path}");
# Ok::<_, staticweaver::EngineError>(())
```

The downloader fetches a fixed set of filenames (`contact.html`, `index.html`, `page.html`, `post.html`, `main.js`, `sw.js`) into a fresh `tempfile::tempdir()`, with a 10 s request timeout and a 1 MiB per-file body cap enforced against both `Content-Length` and the actual read size.

Without the feature, `create_template_folder(Some(url))` returns `EngineError::InvalidTemplate("remote template URLs require the remote-templates feature")`. `create_template_folder(None)` always returns an error — there is no silent default fallback URL.

</details>

---

## Configuration

<details>
<summary><b>Engine construction</b></summary>

```rust
use staticweaver::Engine;
use std::time::Duration;

// Template path + cache TTL.
let mut engine = Engine::new("templates", Duration::from_secs(3600));

// Builder-style global escape opt-out.
let plain = Engine::new("templates", Duration::from_secs(3600))
    .with_html_escape(false);

// Replace delimiters at any time.
engine.set_delimiters("<%", "%>");

// Size-bound the render cache: clears the whole cache if it grows past `max`.
engine.set_max_cache_size(1024);

// Or drop everything now.
engine.clear_cache();
```

</details>

<details>
<summary><b>Cache construction</b></summary>

```rust
use staticweaver::cache::Cache;
use std::time::Duration;

// TTL only, unbounded.
let a: Cache<String, String> = Cache::new(Duration::from_secs(60));

// TTL + initial capacity hint and a hard cap.
let b: Cache<String, String> =
    Cache::with_capacity(Duration::from_secs(60), 1024);
```

Both constructors panic if `ttl` is zero. `Cache::default()` yields a 60 s TTL with no cap.

</details>

---

## Examples

```bash
cargo run --example hello
```

All examples live in `examples/` and use the shared `support.rs` helper for the spinner/checkmark UI. Run any of them with `cargo run --example <name>`.

| Example | Covers |
| :--- | :--- |
| `hello` | Getting started: build an `Engine`, populate a `Context`, render a template |
| `context` | Insert, update, remove, iterate; capacity hints; hash stability |
| `cache` | TTL expiration, capacity bounds, refresh, update, `IntoIterator` |
| `engine` | Escaping defaults, `{{!key}}` opt-out, `render_page`, custom delimiters, path-traversal rejection |
| `errors` | Every `EngineError` / `TemplateError` variant and its conversions |

---

## Development

```bash
make              # check + clippy + test
make build        # cargo build
make test         # cargo test
make lint         # cargo clippy -- -D warnings
make format       # cargo fmt
make check        # cargo check --all-targets
make deny         # cargo deny check
make outdated     # cargo outdated (root package)
make fix          # cargo fix
```

Run `make` with no target to see the full list.

### CI

| Workflow | Trigger | Purpose |
| :--- | :--- | :--- |
| `ci.yml` | push, PR | Clippy, fmt, test, coverage, nightly smoke-test via reusable pipelines |
| `document.yml` | push to main | Build and publish API docs |
| `release.yml` | tag `v*` | Validate, build artifacts, GitHub Release, crates.io publish |

See [CONTRIBUTING.md](CONTRIBUTING.md) for signed commits and PR guidelines.

---

## Security

<details>
<summary><b>Safety guarantees</b></summary>

- `#![forbid(unsafe_code)]` across the entire codebase
- HTML escape on by default for `render_template` / `render_page` substitutions
- `render_page` layout names validated before any filesystem call (rejects `..`, `/`, `\`, null bytes)
- Remote template fetching gated behind an opt-in `remote-templates` cargo feature; default build has no networking
- 10 s timeout and 1 MiB body cap on every remote fetch
- No default third-party URL — `create_template_folder(None)` is an error, not a silent download
- `cargo audit` and `cargo deny` clean (advisories, bans, licenses, sources)
- Yanked crates denied via `[advisories] yanked = "deny"`
- All commits GPG-signed; `Assisted-by:` trailer per the Linux kernel coding-assistants convention
- SPDX license headers on all source files

</details>

---

## License

Dual-licensed under [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT), at your option.

<p align="right"><a href="#staticweaver">Back to Top</a></p>
