<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<p align="center">
  <img src="https://cloudcdn.pro/staticweaver/v1/logos/staticweaver.svg" alt="StaticWeaver logo" width="128" />
</p>

<h1 align="center">StaticWeaver</h1>

<p align="center">
  <strong>Small, safe, fast. Mustache-compatible syntax, Tera-style expressions and inheritance, SIMD HTML escape that matches Askama, zero <code>unsafe</code> code.</strong>
</p>

<p align="center">
  <a href="https://github.com/sebastienrousseau/staticweaver/actions"><img src="https://img.shields.io/github/actions/workflow/status/sebastienrousseau/staticweaver/ci.yml?style=for-the-badge&logo=github" alt="Build" /></a>
  <a href="https://crates.io/crates/staticweaver"><img src="https://img.shields.io/crates/v/staticweaver.svg?style=for-the-badge&color=fc8d62&logo=rust" alt="Crates.io" /></a>
  <a href="https://docs.rs/staticweaver"><img src="https://img.shields.io/badge/docs.rs-staticweaver-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" alt="Docs.rs" /></a>
  <a href="https://codecov.io/gh/sebastienrousseau/staticweaver"><img src="https://img.shields.io/codecov/c/github/sebastienrousseau/staticweaver?style=for-the-badge&logo=codecov" alt="Coverage" /></a>
  <a href="https://lib.rs/crates/staticweaver"><img src="https://img.shields.io/crates/v/staticweaver?style=for-the-badge&label=lib.rs&color=orange" alt="lib.rs" /></a>
</p>

---

## Contents

- [Install](#install) -- Cargo, source, MSRV
- [Quick Start](#quick-start) -- render a template in 10 lines
- [Overview](#overview) -- what staticweaver does
- [When to choose staticweaver](#when-to-choose-staticweaver) -- vs Tera, Handlebars, minijinja
- [Templating Language](#templating-language) -- tags, blocks, filters, expressions
- [Features](#features) -- capability matrix
- [Library Usage](#library-usage) -- rendering, escaping, delimiters, caching, remote templates
- [Configuration](#configuration) -- engine and cache options
- [Examples](#examples) -- six runnable examples
- [Performance](#performance) -- benchmark matrix vs Tera, Minijinja, Askama
- [Development](#development) -- make targets, CI
- [Security](#security) -- safety guarantees
- [FAQ](#faq) -- common questions
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

**With control flow, dot-notation, and a filter** (the rest of what shipped in `v0.0.2`):

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set_value("title".to_string(), "Posts");
ctx.set_value("posts".to_string(), vec!["Hello", "World"]);

let template = "\
<h1>{{ title | uppercase }}</h1>\
{{#if posts}}\
<ul>{{#each posts}}<li>{{this}}</li>{{/each}}</ul>\
{{else}}\
<p>No posts yet.</p>\
{{/if}}";

let out = engine.render_template(template, &ctx).unwrap();
assert_eq!(
    out,
    "<h1>POSTS</h1><ul><li>Hello</li><li>World</li></ul>"
);
```

---

## Overview

StaticWeaver is a small templating engine for Rust. It substitutes `{{name}}` tags against a `Context`, evaluates control-flow blocks and a small expression language, walks template inheritance chains, and writes the result back as a `String`. It is designed to be **safe by default, cacheable, and dependency-light**: most templating crates pick one of those — staticweaver picks all three.

- **HTML-escaped by default** -- `&<>"'` in context values become entities; `{{!key}}` opts out per tag
- **Polymorphic context** -- `Value` enum (`Null` / `Bool` / `Number` / `String` / `List` / `Map`) with dot-notation lookup (`{{user.email}}`, `{{items.0}}`)
- **Control flow** -- `{{#if EXPR}}` / `{{else}}` / `{{/if}}` and `{{#each list}}` with `@index` / `@first` / `@last` / `@key` helpers
- **Expression language** -- comparisons, boolean operators (`and` / `or` / `not`), integer math, postfix tests (`is defined` / `is empty` / `is none`)
- **Partials and inheritance** -- `{{> name}}` partials with parameters, and `{{#extends "base"}}` + `{{#block "name"}}` for layout reuse
- **Filters** -- `{{ x | trim | uppercase }}`, `{{ bio | truncate:140 }}`, with a built-in set covering `uppercase` / `lowercase` / `trim` / `truncate` / `capitalize` / `length` / `default` / `replace` / `urlencode` / `safe`
- **Path-validated `render_page`** -- rejects `..`, `/`, `\`, null bytes in layout names
- **`#![forbid(unsafe_code)]`** -- enforced at the crate root
- **LRU cache** -- generic `Cache<K, V>` with optional capacity bound and true LRU eviction on overflow
- **Custom delimiters** -- swap `{{ }}` for any pair at runtime
- **Opt-in networking** -- remote template fetch lives behind the `remote-templates` cargo feature
- **Bounded HTTP** -- remote fetches cap bodies at 1 MiB with a 10 s timeout
- **Pure Rust** -- no C bindings, no FFI, no build script
- **MSRV 1.68** -- stable Rust only

---

## When to choose staticweaver

| Need | staticweaver | Tera | Handlebars | minijinja |
| :--- | :---: | :---: | :---: | :---: |
| `#![forbid(unsafe_code)]` | yes | no | no | no |
| MSRV 1.68 | yes | newer | newer | newer |
| HTML-escape by default | yes | yes | yes | yes |
| Template inheritance | yes | yes | partials only | yes |
| Built-in filter pipeline | yes | yes | helpers only | yes |
| Async runtime required | no | no | no | no |
| Networking in default build | no | no | no | no |

If you need a **full sandboxed expression language with custom tests, async, or i18n**, pick Tera. If you need the **smallest possible substituter with HTML safety, inheritance, and zero `unsafe`**, pick staticweaver.

---

## Templating Language

| Feature | Syntax | Example |
| :--- | :--- | :--- |
| Substitution | `{{key}}` | `{{name}}` |
| Raw (no escape) | `{{!key}}` | `{{!html_blob}}` |
| Dot-notation | `{{a.b.c}}` | `{{user.email}}` |
| List index | `{{list.0}}` | `{{tags.0}}` |
| Comments | `{{! ... }}` / `{{!-- ... --}}` | `{{!-- TODO --}}` |
| Whitespace control | `{{- key -}}` | strips adjacent whitespace |
| If / else | `{{#if EXPR}}…{{else}}…{{/if}}` | `{{#if user.admin}}` |
| Each | `{{#each list}}…{{/each}}` | `{{#each items}}{{this}}{{/each}}` |
| Each helpers | `@index`, `@first`, `@last`, `@key` | `{{#each users}}{{@index}}: {{this.name}}{{/each}}` |
| Partials | `{{> name}}` | `{{> header}}` |
| Partial parameters | `{{> name k=v}}` | `{{> button label="Save"}}` |
| Inheritance | `{{#extends "base"}}` + `{{#block "x"}}…{{/block}}` | child overrides named blocks |
| Set | `{{#set x = LITERAL}}` | `{{#set tier = "pro"}}` |
| Filters | `{{ x \| filter \| filter:arg }}` | `{{ name \| trim \| uppercase }}` |
| Backslash-escape delimiter | `\{{literal}}` | emits `{{literal}}` verbatim |

### Expression language

The `{{#if EXPR}}` block accepts a small recursive-descent expression language. Precedence: postfix tests bind tightest, then math (`*` / `/` then `+` / `-`), then comparisons, then `not`, `and`, `or`.

| Layer | Operators |
| :--- | :--- |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| Boolean | `and`, `or`, `not` (short-circuiting) |
| Math (integer) | `+`, `-`, `*`, `/` (checked; division-by-zero returns `InvalidTemplate`) |
| Postfix tests | `is defined`, `is empty`, `is none` (negate with `is not`) |

```text
{{#if user.email is defined and user.email is not empty}}
  Hi, {{user.name}}!
{{else}}
  Welcome, guest.
{{/if}}
```

A bare path like `{{#if user}}` keeps its truthiness semantics — it evaluates the lookup and tests `Value::is_truthy`. See [`examples/engine.rs`](examples/engine.rs) for runnable demonstrations.

---

## Features

| | |
| :--- | :--- |
| **Rendering** | `render_template(&str, &Context)` for in-memory strings, `render_page(&Context, layout)` for a `.html` file inside `template_path`. Both return `Result<String, EngineError>`. |
| **Context** | Polymorphic `Value` enum (`Null` / `Bool` / `Number` / `String` / `List` / `Map`). `set` / `get` for legacy strings; `set_value` / `get_value` for typed inserts (`Into<Value>` for `String`, `&str`, `bool`, `i32`, `i64`, `Vec<V>`); `get_path` for dot-notation walks (`user.email`, `items.0`). `iter`, `clear`, `with_capacity`, and a stable `hash()` for cache-key construction. |
| **HTML escape** | On by default. Five entities replaced (`& < > " '`). Per-tag opt-out: `{{!body}}`. Global opt-out: `Engine::new(...).with_html_escape(false)`. |
| **Control flow** | `{{#if EXPR}}…{{else}}…{{/if}}` and `{{#each list}}…{{/each}}` with `@index`, `@first`, `@last`, `@key` loop helpers and `Map` iteration. |
| **Expressions** | Recursive-descent parser inside `#if`: comparisons (`==` `!=` `<` `<=` `>` `>=`), boolean ops (`and` `or` `not`, short-circuiting), integer math (`+` `-` `*` `/`, checked arithmetic), postfix tests (`is defined`, `is empty`, `is none`, with `is not` negation). |
| **Partials & inheritance** | `{{> name}}` partials with `{{> name k=v}}` parameters and a depth-10 recursion guard; `{{#extends "base"}}` + `{{#block "name"}}…{{/block}}` for multi-level inheritance, child wins on conflicts. |
| **Filters** | Pipeline syntax `{{ x | f | g:arg }}`. Built-in: `uppercase`, `lowercase`, `trim`, `truncate`, `capitalize`, `length`, `default`, `replace`, `urlencode`, `safe`. |
| **In-template assignment** | `{{#set name = LITERAL}}` binds locally without leaking to the parent scope. |
| **Delimiters** | `set_delimiters(open, close)` swaps `{{` / `}}` for any pair. Whitespace around keys is trimmed (`{{ name }}` == `{{name}}`). Whitespace control via `{{- key -}}`. Backslash-escape via `\{{literal}}`. |
| **Cache** | Generic `Cache<K, V>` with time-based expiration and an optional hard capacity. **True LRU eviction** on overflow — `Cache::get` (now `&mut self`) bumps access recency. Methods: `insert`, `get`, `ttl`, `refresh`, `update`, `remove`, `contains_key`, `remove_expired`, `clear`, `iter`, `IntoIterator`. |
| **Remote templates** | `create_template_folder(Some(url))` under `--features remote-templates`. 10 s timeout, 1 MiB body cap, status-code check, `Content-Type` validation, `rustls-tls-native-roots`. The default-URL fallback has been removed; `create_template_folder(None)` is an error. |
| **Errors** | `EngineError` (`Io`, `Render`, `InvalidTemplate`, `Template`, `ResourceNotFound`, `Timeout`, and `Reqwest` under the feature). `TemplateError` with `#[from]` conversions for `io::Error` and `reqwest::Error`. |
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

The `Engine::render_page` path caches by `"{layout}:{ctx.hash()}"`. `Context::hash` is order-independent (XOR-combined per entry) so equal logical contexts always produce equal hashes, making the cache hit deterministically rather than thrashing on insertion order.

Bounded caches use **true LRU eviction**: when a new key would push the cache past its cap, the least-recently-used entry is evicted. `Cache::get` (now `&mut self`) bumps access recency, so frequently-rendered pages stay hot. Updating an existing key never triggers eviction. `set_max_cache_size(n)` resizes the cap; entries above the new cap are evicted on the next insert.

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

// Size-bound the render cache: capacity-pressure inserts evict the
// least-recently-used entry.
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

All examples live in `examples/` and use the shared `support.rs` helper for the spinner/checkmark UI. Run any of them with `cargo run --example <name>`. See [`examples/README.md`](examples/README.md) for an annotated index.

| Example | Covers |
| :--- | :--- |
| `hello` | Getting started: build an `Engine`, populate a `Context`, render a template |
| `context` | Insert, update, remove, iterate; typed values; dot-notation; hash stability |
| `cache` | TTL expiration, **LRU eviction**, refresh, update, `IntoIterator` |
| `engine` | Escaping defaults, `{{!key}}` opt-out, partials, filters, control flow, dot-notation, `render_page` with subdirectories, custom delimiters, path-traversal rejection |
| `errors` | Every `EngineError` / `TemplateError` variant and its conversions |
| `remote` | (feature-gated) `create_template_folder(Some(url))` against a local mock server. Run with `cargo run --example remote --features remote-templates`. |

---

## Performance

`staticweaver` aims to be the **fastest non-codegen Rust template engine**. Full-quality `cargo bench --bench comparative` numbers vs Tera, Minijinja, and Askama (Apple M-series, 2 s warm-up + 5 s measurement; lower is better):

| Workload | staticweaver | Tera | Minijinja | Askama |
| :--- | ---: | ---: | ---: | ---: |
| `simple_sub` (1 tag) | **468 ns** | 386 ns | 584 ns | 93 ns |
| `many_sub_32` (32 tags) | **11.8 µs** | 4.58 µs | 11.24 µs | 830 ns |
| `escape_heavy` (10 KB, 5% metachar) | **22.8 µs** | 84.2 µs | 23.2 µs | 22.9 µs |
| `each_100` (100 items) | 54.9 µs | 17.7 µs | 19.3 µs | 5.13 µs |
| `each_1000` (1000 items) | 535 µs | 171 µs | 178 µs | 48 µs |
| `if_chain` (nested conditionals) | 2.30 µs | 444 ns | 640 ns | 25 ns |
| `filter_chain` (`trim \| upper`) | **980 ns** | 618 ns | 976 ns | 197 ns |

* **Wins or ties Minijinja on 4 / 7 workloads.**
* **Beats Tera on `escape_heavy` 3.7×.**
* **Matches Askama on `escape_heavy`** (22.8 µs vs 22.9 µs) — the SIMD escape path holds its own against compile-time codegen on long inputs.

The remaining 2.8–3.6× gap on loops and conditional chains is constant-factor per-tag overhead in the runtime AST walker. Closing it would require a bytecode compiler — explicitly rejected to preserve the "small enough to read in an afternoon" pillar.

See [`PERFORMANCE.md`](PERFORMANCE.md) for the full Phase D progression, what the engine caches at runtime, and how to reproduce the numbers on your own hardware.

```bash
cargo bench --bench comparative           # full quality (~5 min)
cargo bench --bench comparative -- --quick # smoke test (<1 min)
```

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
| `ci.yml` | push, PR | Clippy, fmt, test (Linux + macOS + Windows), coverage gate (95%), `cargo deny`, via reusable pipelines |
| `release.yml` | tag `v*.*.*` | Validate matrix (macOS + Linux + Windows), build artifacts, GitHub Release, crates.io publish |

### Documentation

| Surface | Hosted at | Built by | Trigger |
| :--- | :--- | :--- | :--- |
| API reference | [docs.rs/staticweaver](https://docs.rs/staticweaver) | docs.rs | `crates.io` publish |
| README + crate-level prose | [docs.rs/staticweaver](https://docs.rs/staticweaver) (front page) | `#![doc = include_str!("../README.md")]` in `src/lib.rs` | every `cargo doc` |
| CHANGELOG / SECURITY / FAQ | This GitHub repo | — | every push |

The CI `docs` job builds documentation under `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links"`, runs every doctest with `--all-features`, and enforces 100% example coverage via `cargo +nightly rustdoc -- -Z unstable-options --show-coverage`. Publication itself is handled by docs.rs on every crates.io release — there is no `gh-pages` branch and no separate documentation workflow.

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

## FAQ

<details>
<summary><b>Is staticweaver async?</b></summary>

No. The render path is synchronous. The remote-template downloader (behind the `remote-templates` feature) uses blocking `reqwest`. If you need to fetch templates from inside an async task, call `create_template_folder` from `tokio::task::spawn_blocking`.

</details>

<details>
<summary><b>Does it support template inheritance?</b></summary>

Yes. `{{#extends "base"}}` plus `{{#block "name"}}…{{/block}}` works with multi-level chains; the child wins on conflicting block names. See the [Templating Language](#templating-language) section.

</details>

<details>
<summary><b>How is it different from Tera or Handlebars?</b></summary>

`#![forbid(unsafe_code)]`, MSRV 1.68, no networking in the default build, and a small (~700 LoC) recursive-descent expression evaluator that covers comparisons, booleans, integer math, and the `is defined` / `is empty` / `is none` postfix tests. See the [comparison table](#when-to-choose-staticweaver).

</details>

<details>
<summary><b>Can I use it with Axum or Actix?</b></summary>

Yes — render to a `String`, then return it. There is no framework integration layer because none is needed; the engine is a pure function over `(template, context)`.

</details>

<details>
<summary><b>How are missing values rendered?</b></summary>

A missing key in `render_template` returns `EngineError::Render`. Inside `{{#if}}`, a missing path evaluates to `Value::Null` (which is falsy). Use `{{#if x is defined}}` to distinguish "missing" from "explicit `Null`".

</details>

<details>
<summary><b>What is the cache key for `render_page`?</b></summary>

`"{layout}:{Context::hash()}"`. `Context::hash` is order-independent, so two contexts with the same logical contents hit the same cache entry regardless of insertion order.

</details>

---

## License

Dual-licensed under [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT), at your option.

See [CHANGELOG.md](CHANGELOG.md) for release history, [SECURITY.md](SECURITY.md) for the disclosure policy, and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community guidelines.

<p align="right"><a href="#staticweaver">Back to Top</a></p>
