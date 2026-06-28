<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<p align="center">
  <img src="https://cloudcdn.pro/staticweaver/v1/logos/staticweaver.svg" alt="StaticWeaver logo" width="128" />
</p>

<h1 align="center">Static Weaver (staticweaver)</h1>

<p align="center">
  <strong>Tera-tier templating in bytes. Pure Rust. Zero <code>unsafe</code>. Idempotent HTML escape. <code>Send + Sync</code> engine with an optional async surface.</strong>
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
- [Ecosystem Comparison](#ecosystem-comparison) -- vs Tera, Minijinja, Handlebars, Askama
- [Benchmarks](#benchmarks) -- comparative + internal regression matrix
- [Templating Language](#templating-language) -- tags, blocks, filters, expressions
- [Features](#features) -- capability matrix
- [Library Usage](#library-usage) -- rendering, escaping, loaders, filters, streaming
- [Configuration](#configuration) -- engine and cache options
- [Examples](#examples) -- 11 runnable examples
- [Development](#development) -- make targets, fuzzing, CI
- [Security](#security) -- safety guarantees
- [FAQ](#faq) -- common questions
- [License](#license)

---

## Install

```toml
[dependencies]
staticweaver = "0.0.4"
```

### Optional features

All optional integrations are off by default. Enable only what the application needs.

```toml
[dependencies]
staticweaver = { version = "0.0.4", features = ["async-tokio", "tracing", "json"] }
```

| Feature | Pulls in | Adds |
| :--- | :--- | :--- |
| `async` | — | `AsyncTemplateLoader` trait (AFIT) + `Engine::render_{template,page,to,page_to}_async`. Runtime-agnostic. |
| `async-tokio` | `tokio` (`rt-multi-thread`, `fs`) | `TokioFsLoader` non-blocking impl; `AsyncWrite` sinks. Implies `async`. |
| `tracing` | `tracing` | `#[tracing::instrument]` spans on `render_template` / `render_page` (`staticweaver.render_template`, `staticweaver.render_page`). |
| `json` | `serde_json` | `{{ value \| json \| safe }}` filter. |
| `remote-templates` | `reqwest` (`rustls-native-certs`, no OpenSSL) | `Engine::create_template_folder(Some(url))` HTTP fetcher (10 s timeout, 1 MiB body cap, `Content-Type` validation). |
| `axum-example` | `axum`, `tokio` | Compiles `examples/axum.rs`. |

### Build from source

```bash
git clone https://github.com/sebastienrousseau/staticweaver.git
cd staticweaver
make          # check + clippy + test
```

**MSRV: Rust 1.75.0+** (raised in v0.0.4 for async fn in traits used by the optional `async` feature). Tested on Linux, macOS, and Windows.

---

## Quick Start

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("title".to_string(), "Posts".to_string());
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

staticweaver is designed to be the **no-compromise** templating engine for Rust: small, safe, fast, and ergonomic — simultaneously. Most engines pick one (small but limited, or full-featured but heavy). staticweaver picks all four.

- **Pure Rust** -- no C bindings, no FFI, no `unsafe` blocks (`#![forbid(unsafe_code)]`)
- **Mustache-compatible substitution** -- `{{name}}` syntax with HTML-escape by default
- **Idempotent HTML escape** -- `escape(escape(x)) == escape(x)`, proptest- and Kani-defended
- **Tera-style expressions** -- comparisons, boolean ops, integer math, postfix tests
- **Template inheritance** -- `{{#extends}}` + `{{#block}}` with `{{ super() }}`
- **23 built-in filters** -- pipeline syntax `{{ x | trim | uppercase }}`
- **Custom filters + tests** -- runtime extension via `Engine::add_filter` / `add_test`
- **Pluggable loaders** -- `TemplateLoader` trait; built-in `FsLoader`, `MemoryLoader`
- **`Send + Sync` engine** (v0.0.4) -- share one `Arc<Engine>` across threads / async tasks; render cache lives behind `std::sync::Mutex` internally
- **Optional async surface** (v0.0.4) -- `AsyncTemplateLoader` trait + `Engine::render_{template,page,to,page_to}_async` methods under the `async-tokio` feature
- **Optional `tracing` integration** (v0.0.4) -- `#[tracing::instrument]` spans on the render hot path
- **LRU cache** -- TTL + true LRU eviction; deterministic, collision-safe cache keys
- **Stream rendering** -- `render_to(template, ctx, &mut io::Write)` (sync) and `render_to_async` (async) for HTTP body sinks
- **Line:column errors** -- every error carries `at line N, column M`
- **CLI binary** -- `cargo install staticweaver` ships a shell-side renderer
- **Property-based + differential testing** -- 1500 random inputs/run, byte-equality vs Minijinja
- **Coverage-guided fuzzing** -- three libfuzzer targets nightly (`parse`, `escape`, `dot_path`)
- **Formal verification** -- Kani proofs of `html_escape` idempotency
- **4 runtime dependencies** -- `fnv`, `tempfile`, `thiserror` (`reqwest`, `tokio`, `tracing`, `serde_json`, `axum` are all optional)
- **610+ tests** -- unit, integration, doctest, snapshot, differential, proptest, concurrent, async
- **12 runnable examples** with animated spinner UI (including `async_tokio` and `axum`)

---

## Ecosystem Comparison

staticweaver competes across four categories of Rust templating engines:

| | staticweaver | Tera | Minijinja | Handlebars | Askama |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Pure Rust** | Yes | Yes | Yes | Yes | Yes |
| **Zero `unsafe`** (`#![forbid(unsafe_code)]`) | Yes | No | No | No | No |
| **MSRV** | 1.75 | newer | newer | newer | newer |
| **`Send + Sync` engine** | Yes (v0.0.4) | Partial | Partial | Partial | n/a |
| **Async loader / streaming** | Yes (`async-tokio` feature) | No | No | No | No |
| **Formal verification (Kani)** | Yes (escape idempotency) | No | No | No | No |
| **Coverage-guided fuzz** | Yes (3 targets) | No | No | No | No |
| **SBOM + Sigstore on release** | Yes | No | No | No | No |
| **HTML-escape by default** | Yes | Yes | Yes | Yes | Yes |
| **Compile-time codegen** | No | No | No | No | Yes |
| **Runtime template loading** | Yes | Yes | Yes | Yes | No |
| **Template inheritance** | Yes | Yes | Yes | Partial | Yes |
| **`{{ super() }}` in blocks** | Yes | Yes | Yes | No | Yes |
| **Built-in filter pipeline** | 23 filters | Yes | Yes | Helpers | Yes |
| **Custom filters at runtime** | Yes | Yes | Yes | Yes | No |
| **Custom tests at runtime** | Yes | Yes | Yes | No | No |
| **Pluggable template loader** | `TemplateLoader` | Partial | Yes | Yes | No |
| **Stream rendering (`io::Write`)** | Yes | Yes | Yes | Yes | Yes |
| **Line:column in errors** | Yes | Yes | Yes | Partial | Yes |
| **Range iteration (`1..N`)** | Yes | Yes | Yes | No | No |
| **`#break` / `#continue`** | Yes | Yes | Yes | No | No |
| **CLI binary** | Yes | No | Partial | No | No |
| **Idempotent HTML escape** | Yes (`escape(escape(x))==escape(x)`) | No | No | No | No |
| **Differential vs Minijinja** | 9 tests | No | -- | No | No |
| **Property-based fuzzing** | proptest | No | No | No | No |
| **Async runtime required** | No | No | No | No | No |
| **Networking in default build** | No | No | No | No | No |

---

## Benchmarks

Benchmarked on Apple M-series, Rust stable. All libraries compiled with `--release` via Criterion (2 s warm-up + 5 s measurement).

### Comparative throughput (lower is better)

| Workload | staticweaver | Tera | Minijinja | Askama |
| :--- | ---: | ---: | ---: | ---: |
| `simple_sub` (1 tag) | **497 ns** | 388 ns | 591 ns | 95 ns |
| `many_sub_32` (32 tags) | **12.85 µs** | 5.96 µs | 14.40 µs | 973 ns |
| `escape_heavy` (10 KB body, 5% metachar) | 26.2 µs† | 77.8 µs | 24.3 µs | 23.2 µs |
| `each_100` | 58.3 µs | 17.8 µs | 23.6 µs | 5.24 µs |
| `each_1000` | 557 µs | 171 µs | 184 µs | 51.9 µs |
| `if_chain` (nested conditionals) | 2.51 µs | 455 ns | 656 ns | 25.4 ns |
| `filter_chain` (`trim \| upper`) | **1.03 µs** | 620 ns | 988 ns | 198 ns |

**Wins or ties Minijinja on 4/7 workloads.** Beats Tera 3.0× on `escape_heavy`. † `escape_heavy` re-measured in v0.0.3 after `askama_escape` was removed in service of the ssg#589 idempotency invariant: the scalar entity-aware fast path now lands within ~12.5 % of the dropped SIMD baseline (was 23.3 µs). See [`PERFORMANCE.md`](PERFORMANCE.md).

### Internal regression guards

| Bench | Time | Insight |
| :--- | ---: | :--- |
| `render_template_escape_heavy` | 26.2 µs | Inline byte-indexed entity-aware fast path (v0.0.3) |
| `render_page_cache_hit` | 2.12 µs | Warmed-cache fast path |
| `render_page_cache_miss` | 14.20 µs | Cold render + parse + load |
| `render_inheritance_with_super` | 415 ns | 3-layout merge with `{{ super() }}` |
| `render_partial_in_each_100` | 93.6 µs | 100 partial includes (~940 ns each) |
| `render_to_vec` vs `render_template_to_string` | 11.71 µs vs 11.65 µs | Streaming parity (+0.5%, in variance) |
| `filter_dispatch_custom_uppercase` vs `_builtin` | 828 ns vs 800 ns | +3.4% override overhead |
| `filter_chain_five_filters` vs `_one_filter` | 1.99 µs vs 849 ns | Linear in N |

### Architecture validation

| Capability | Measured Impact |
| :--- | :--- |
| LRU cache hit vs miss | **6.7×** faster on a hit (2.12 µs vs 14.20 µs) |
| Inline byte-indexed escape fast path (v0.0.3, #33) | **−66.6%** vs scalar `char_indices` after `askama_escape` removal (78.3 µs → 26.2 µs) |
| `#each` clone hoisted out of loop body | **35×** on each_1000 (22.6 ms → 640 µs) |
| `set_value_string` reuses heap buffer | **−18%** on each_100 |
| `Context::hash` (sort-then-hash, order-independent, collision-safe in v0.0.3) | O(n log n), zero allocation in the hot path |

Reproduce: `cargo bench --bench comparative` and `cargo bench --bench template_benchmark`. See [`PERFORMANCE.md`](PERFORMANCE.md) for the full Phase D progression.

### Project metrics

| Metric | Value |
| :--- | :--- |
| **Source** | ~10.7k lines across 6 modules (engine, context, cache, error, loader_async, lib) |
| **Test suite** | 610+ tests + 101 doctests + 1500 random proptest cases / run + 3 libfuzzer targets + 2 Kani proofs |
| **Coverage** | 98% line-coverage floor enforced in CI / 100% rustdoc with examples (compile-time-enforced via `missing_docs = "deny"`) |
| **Examples** | 12 branded examples (`hello`, `context`, `cache`, `engine`, `errors`, `remote`, `inheritance`, `filters`, `loaders`, `control_flow`, `async_tokio`, `axum`) |
| **Benchmarks** | 14 internal regression guards + 7-workload comparative matrix |
| **Dependencies** | 3 runtime (`fnv`, `tempfile`, `thiserror`) + 5 optional (`reqwest`, `serde_json`, `tokio`, `tracing`, `axum`) |
| **MSRV** | Rust 1.75.0 |

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
| Each (range) | `{{#each START..END}}…{{/each}}` | `{{#each 1..10}}{{this}}{{/each}}` |
| Each helpers | `@index`, `@first`, `@last`, `@key` | `{{#each users}}{{@index}}: {{this.name}}{{/each}}` |
| Loop control | `{{#break}}` / `{{#continue}}` | exit / skip current iteration |
| Partials | `{{> name}}` | `{{> header}}` |
| Partial parameters | `{{> name k=v}}` | `{{> button label="Save"}}` |
| Inheritance | `{{#extends "base"}}` + `{{#block "x"}}…{{/block}}` | child overrides named blocks |
| `super()` in override | `{{ super() }}` | include parent block body |
| Set | `{{#set x = LITERAL}}` | `{{#set tier = "pro"}}` |
| Filters | `{{ x \| filter \| filter:arg }}` | `{{ name \| trim \| uppercase }}` |
| Backslash-escape delimiter | `\{{literal}}` | emits `{{literal}}` verbatim |

### Expression language

The `{{#if EXPR}}` block accepts a small recursive-descent expression language. Precedence: postfix tests bind tightest, then math (`*` / `/` then `+` / `-`), then string concat (`~`), then comparisons, then `not`, `and`, `or`.

| Layer | Operators |
| :--- | :--- |
| Postfix tests | `is defined`, `is empty`, `is none` (negate with `is not`); register your own with `Engine::add_test` |
| Math (integer) | `+`, `-`, `*`, `/` (checked; division-by-zero returns `InvalidTemplate`) |
| String concat | `~` (Tera/Twig style) — `name ~ " Lovelace"` |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| Boolean | `and`, `or`, `not` (short-circuiting) |

```text
{{#if user.email is defined and user.email is not empty}}
  Hi, {{user.name}}!
{{else}}
  Welcome, guest.
{{/if}}
```

A bare path like `{{#if user}}` keeps its truthiness semantics — it evaluates the lookup and tests `Value::is_truthy`.

---

## Features

| | |
| :--- | :--- |
| **Rendering** | `render_template(&self, &str, &Context)` and `render_page(&self, &Context, layout)` return `Result<String, EngineError>`; `render_to<W: io::Write>` and `render_page_to<W: io::Write>` stream into any sink (HTTP body, file, channel). All take `&self` (v0.0.4) so one `Arc<Engine>` is shareable across threads. Async mirrors (`render_template_async`, `render_page_async`, `render_to_async`, `render_page_to_async`) live under the `async-tokio` feature. |
| **Context** | Polymorphic `Value` enum (`Null` / `Bool` / `Number` / `String` / `List` / `Map`). `set` / `get` for legacy strings; `set_value` / `set_value_str` / `set_value_string` / `get_value` for typed inserts (`Into<Value>` for `String`, `&str`, `bool`, `i32`, `i64`, `Vec<V>`); `get_path` for dot-notation walks (`user.email`, `items.0`). `iter`, `clear`, `with_capacity`, and a stable `hash()` for cache-key construction. |
| **HTML escape** | Inline byte-indexed scan over the OWASP 5-character set (`& < > " '`) with bulk-flush via `push_str`. On by default. **Idempotent** (`escape(escape(x)) == escape(x)`) — proptest- and Kani-defended; already-formed entity references (`&amp;`, `&#169;`, `&#xA9;`, named refs up to 31 chars) are preserved. Per-tag opt-out: `{{!body}}` or trailing ` \| safe` filter. Global opt-out: `Engine::new(...).with_html_escape(false)`. Per-extension policy: `engine.autoescape_on(&[".html", ".xml"])`. |
| **Control flow** | `{{#if EXPR}}…{{else}}…{{/if}}` and `{{#each list}}…{{/each}}` with `@index`, `@first`, `@last`, `@key` loop helpers, `Map` iteration, range form (`{{#each START..END}}`), and `{{#break}}` / `{{#continue}}` early-exit tags. |
| **Expressions** | Recursive-descent parser inside `#if`: comparisons (`==` `!=` `<` `<=` `>` `>=`), boolean ops (`and` `or` `not`, short-circuiting), integer math (`+` `-` `*` `/`, checked arithmetic), string concat (`~`), postfix tests (`is defined`, `is empty`, `is none`, with `is not` negation; user-extensible via `Engine::add_test`). |
| **Partials & inheritance** | `{{> name}}` partials with `{{> name k=v}}` parameters and a depth-10 recursion guard; `{{#extends "base"}}` + `{{#block "name"}}…{{/block}}` for multi-level inheritance (child wins on conflicts), with `{{ super() }}` to include the parent block body inside an override. |
| **Filters** | Pipeline syntax `{{ x \| f \| g:arg }}`. **23 built-in filters**: `uppercase`, `lowercase`, `trim`, `truncate`, `capitalize`, `length`, `default`, `replace`, `urlencode`, `safe`, `abs`, `round`, `ceil`, `floor`, `number_format`, `repeat`, `reverse`, `slice`, `pad_start`, `pad_end`, `contains`, `starts_with`, `ends_with`. `json` available under `--features json`. Register your own via `Engine::add_filter("name", Arc::new(…))`. |
| **Custom tests** | `Engine::add_test("admin", Arc::new(\|v, args\| Ok(…)))` registers user predicates for `is X` / `is not X`. Custom tests override built-in `defined`/`empty`/`none` of the same name. |
| **Template loaders** | `TemplateLoader` trait with built-in `FsLoader` (default) and `MemoryLoader` (testing/embedded assets). Plug in your own backend via `Engine::with_loader(Arc::new(MyLoader), ttl)`. |
| **In-template assignment** | `{{#set name = LITERAL}}` binds locally without leaking to the parent scope. |
| **Delimiters** | `set_delimiters(open, close)` swaps `{{` / `}}` for any pair. Whitespace around keys is trimmed. Whitespace control via `{{- key -}}`. Backslash-escape via `\{{literal}}`. |
| **Cache** | Generic `Cache<K, V>` with time-based expiration and an optional hard capacity. **True LRU eviction** on overflow — `Cache::get` (`&mut self`) bumps access recency. Methods: `insert`, `get`, `ttl`, `refresh`, `update`, `remove`, `contains_key`, `remove_expired`, `clear`, `iter`, `IntoIterator`. **Observability**: `CacheStats { inserts, hits, misses, evictions, ttl_expired }` via `Cache::stats()` for cheap export to prometheus / metrics / opentelemetry without forcing a dep choice. On the `Engine` the cache lives behind `std::sync::Mutex` so `render_page` can take `&self`. |
| **Remote templates** | `create_template_folder(Some(url))` under `--features remote-templates`. 10 s timeout, 1 MiB body cap, status-code check, `Content-Type` validation, `rustls-native-certs`. The default-URL fallback has been removed; `create_template_folder(None)` is an error. |
| **Errors** | `EngineError` (`Io`, `Render`, `InvalidTemplate`, `Template`, `ResourceNotFound`, `Timeout`, and `Reqwest` under the feature). All user-facing messages carry `at line N, column M` for source positions. `TemplateError` with `#[from]` conversions for `io::Error` and `reqwest::Error`. |
| **CLI** | `cargo install staticweaver` ships a `staticweaver` binary: `staticweaver render <template> [--set k=v ...] [--no-escape]`. Reads templates from a file path or stdin (`-`). |
| **Robustness** | proptest harness (256 random cases × 9 properties = ~2300 inputs per `cargo test`) proving the engine never panics on arbitrary input. Differential tests against Minijinja anchor the shared-syntax contract. Three nightly libfuzzer targets (`parse`, `escape`, `dot_path`) cover the bytes proptest misses. Two `#[cfg(kani)]` formal proofs against `escape_html_into` (idempotency + no bare metachars). |
| **Supply chain** | `cargo deny` (advisories, bans, licenses, sources — yanked denied) + `cargo audit` (CVSS gate) + `osv-scanner` (GHSA + RUSTSEC + crates.io) + `cargo vet` (Mozilla / Google / bytecode-alliance / Embark imports). Release artifacts: CycloneDX + SPDX JSON SBOMs, cosign keyless OIDC signatures (Sigstore), all attached to the GitHub Release. |
| **Observability** | `tracing` feature (opt-in) adds `#[tracing::instrument]` spans on `render_template` / `render_page` with template-length / layout / context.len fields. Wire to `tracing_subscriber`, `tracing-opentelemetry`, or any other consumer. |

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

let safe = engine.render_template("Hi {{user}}", &ctx).unwrap();
assert_eq!(safe, "Hi &lt;script&gt;alert(1)&lt;/script&gt;");

let raw = engine.render_template("{{!body}}", &ctx).unwrap();
assert_eq!(raw, "<b>hi</b>");

let plain = Engine::new("", Duration::from_secs(60))
    .with_html_escape(false)
    .render_template("{{body}}", &ctx)
    .unwrap();
assert_eq!(plain, "<b>hi</b>");
```

Only the substituted values are escaped — template text is emitted verbatim, so `<h1>` in the template stays as `<h1>`.

</details>

<details>
<summary><b>Page rendering (file-backed templates)</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let mut engine = Engine::new("templates", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("title".to_string(), "About".to_string());

let _result = engine.render_page(&ctx, "layout");
```

`render_page` rejects layout names containing `/`, `\`, null bytes, or a `..` segment before touching the filesystem, so `render_page(&ctx, "../../etc/passwd")` returns `EngineError::InvalidTemplate` rather than reading outside the template directory.

</details>

<details>
<summary><b>Stream rendering into an `io::Write` sink</b></summary>

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set("name".to_string(), "Ada".to_string());

let mut buf: Vec<u8> = Vec::new();
engine.render_to("Hello, {{name}}!", &ctx, &mut buf).unwrap();
assert_eq!(buf, b"Hello, Ada!");
```

`render_to` writes directly to any `io::Write` — `Vec<u8>`, an HTTP response body, a `File`, a `tokio` channel writer. Saves the `String → Vec<u8>` step in the framework's `IntoResponse` path. `render_page_to(&ctx, layout, &mut writer)` is the file-backed counterpart.

</details>

<details>
<summary><b>Custom filters and tests</b></summary>

```rust
use staticweaver::{Context, Engine};
use staticweaver::context::Value;
use std::sync::Arc;
use std::time::Duration;

let mut engine = Engine::new("", Duration::from_secs(60));

engine.add_filter(
    "shout",
    Arc::new(|input, _args| Ok(format!("{}!!!", input.to_uppercase()))),
);

engine.add_test(
    "admin",
    Arc::new(|v: &Value, _args| {
        Ok(matches!(v, Value::String(s) if s == "admin"))
    }),
);

let mut ctx = Context::new();
ctx.set("name".to_string(), "ada".to_string());
ctx.set("role".to_string(), "admin".to_string());

let out = engine
    .render_template(
        "{{name | shout}} - {{#if role is admin}}Y{{else}}N{{/if}}",
        &ctx,
    )
    .unwrap();
assert_eq!(out, "ADA!!! - Y");
```

Both `add_filter` and `add_test` register `Arc<Fn>` closures that override built-ins of the same name. Errors flow through as `EngineError::Render`.

</details>

<details>
<summary><b>Pluggable template loaders</b></summary>

```rust
use staticweaver::{Context, Engine};
use staticweaver::engine::MemoryLoader;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

let mut store = HashMap::new();
let _ = store.insert("greet".to_string(), "Hi, {{name}}!".to_string());
let mut engine = Engine::with_loader(
    Arc::new(MemoryLoader::new(store)),
    Duration::from_secs(60),
);

let mut ctx = Context::new();
ctx.set("name".to_string(), "Ada".to_string());
assert_eq!(engine.render_page(&ctx, "greet").unwrap(), "Hi, Ada!");
```

`Engine::with_loader(Arc<dyn TemplateLoader>, ttl)` substitutes any `Send + Sync` loader for the default filesystem-backed `FsLoader`. Implement `TemplateLoader` yourself to load templates from a database, an embedded asset bundle, or a remote service.

</details>

<details>
<summary><b>Per-extension auto-escape policy</b></summary>

```rust
use staticweaver::{Context, Engine};
use staticweaver::engine::MemoryLoader;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

let mut store = HashMap::new();
let _ = store.insert("page.html".to_string(), "{{x}}".to_string());
let _ = store.insert("plain.txt".to_string(), "{{x}}".to_string());

let mut engine = Engine::with_loader(
    Arc::new(MemoryLoader::new(store)),
    Duration::from_secs(60),
);
let _ = engine.autoescape_on(&[".html"]);

let mut ctx = Context::new();
ctx.set("x".to_string(), "<b>".to_string());

assert_eq!(engine.render_page(&ctx, "page.html").unwrap(), "&lt;b&gt;");
assert_eq!(engine.render_page(&ctx, "plain.txt").unwrap(), "<b>");
```

`autoescape_on(&[".html", ".xml"])` makes `render_page` auto-escape only for layouts whose name ends with one of the listed extensions. The global `escape_html` flag still applies to `render_template`.

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

cache.remove_expired();
```

`Engine::render_page` caches by `"{layout}:{ctx.hash()}"`. `Context::hash` is order-independent (XOR-combined per entry) so equal logical contexts always produce equal hashes.

Bounded caches use **true LRU eviction**: when a new key would push the cache past its cap, the least-recently-used entry is evicted. `Cache::get` bumps access recency, so frequently-rendered pages stay hot. `set_max_cache_size(n)` resizes the cap; entries above the new cap are evicted on the next insert.

</details>

<details>
<summary><b>Remote templates (feature-gated)</b></summary>

```toml
[dependencies]
staticweaver = { version = "0.0.4", features = ["remote-templates"] }
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

The downloader fetches a fixed set of filenames into a fresh `tempfile::tempdir()`, with a 10 s request timeout and a 1 MiB per-file body cap enforced against both `Content-Length` and the actual read size.

Without the feature, `create_template_folder(Some(url))` returns `EngineError::InvalidTemplate`. `create_template_folder(None)` always returns an error — there is no silent default fallback URL.

</details>

<details>
<summary><b>JSON encode filter (feature-gated)</b></summary>

```toml
[dependencies]
staticweaver = { version = "0.0.4", features = ["json"] }
```

```rust,ignore
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("", Duration::from_secs(60));
let mut ctx = Context::new();
ctx.set_value("items".to_string(), vec!["a", "b"]);

let out = engine
    .render_template("{{ items | json | safe }}", &ctx)
    .unwrap();
assert_eq!(out, r#"["a","b"]"#);
```

The `json` filter walks the full `Value` tree and serialises via `serde_json`. Map keys are sorted for deterministic output. Pair with `| safe` inside `<script>` blocks to suppress the engine's HTML escape on the JSON text.

</details>

<details>
<summary><b>Concurrent rendering with <code>Arc&lt;Engine&gt;</code> (v0.0.4)</b></summary>

`Engine` is `Send + Sync + Clone` as of v0.0.4. Share one across threads — the render cache lives behind `std::sync::Mutex` internally; the lock is held only across cache lookup / insert, so the expensive render work happens in parallel.

```rust
use staticweaver::engine::MemoryLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = HashMap::new();
    let _ = store.insert("page".to_string(), "hi {{name}}".to_string());
    let engine = Arc::new(Engine::with_loader(
        Arc::new(MemoryLoader::new(store)),
        Duration::from_secs(60),
    ));

    let mut handles = Vec::new();
    for id in 0..4 {
        let engine = Arc::clone(&engine);
        handles.push(thread::spawn(move || {
            let mut ctx = Context::new();
            ctx.set("name".to_string(), format!("worker-{id}"));
            // `&self` — no `Arc<Mutex<Engine>>` envelope required.
            engine.render_page(&ctx, "page").unwrap()
        }));
    }
    for h in handles {
        println!("{}", h.join().unwrap());
    }
    Ok(())
}
```

</details>

<details>
<summary><b>Async rendering with the <code>async-tokio</code> feature (v0.0.4)</b></summary>

Requires `staticweaver = { version = "0.0.4", features = ["async-tokio"] }`.

```rust,ignore
use staticweaver::loader_async::MemoryAsyncLoader;
use staticweaver::{Context, Engine};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build an async-loader-backed engine.
    let mut store = HashMap::new();
    let _ = store
        .insert("greeting".to_string(), "Hi, {{name}}!".to_string());
    let loader = MemoryAsyncLoader::new(store);
    let engine = Engine::new("", Duration::from_secs(60));

    let mut ctx = Context::new();
    ctx.set("name".to_string(), "Ada".to_string());

    // Async load + sync render; cache shared with sync render_page.
    let out = engine
        .render_page_async(&loader, &ctx, "greeting")
        .await?;
    assert_eq!(out, "Hi, Ada!");

    // Async streaming into any AsyncWrite sink (e.g. tokio::fs::File,
    // Axum body channel, tokio::net::TcpStream).
    let mut sink: Vec<u8> = Vec::new();
    engine
        .render_to_async("hello {{name}}", &ctx, &mut sink)
        .await?;
    assert_eq!(&sink, b"hello Ada");
    Ok(())
}
```

`TokioFsLoader::new(path)` provides a non-blocking filesystem loader. Implement `AsyncTemplateLoader` for KV stores, embedded asset bundles, HTTP CDNs — the trait uses async fn in traits (AFIT), MSRV 1.75.

</details>

<details>
<summary><b><code>tracing</code> spans on the render hot path</b></summary>

Requires `staticweaver = { version = "0.0.4", features = ["tracing"] }`.

```rust,ignore
use staticweaver::{Context, Engine};
use std::time::Duration;
use tracing_subscriber::fmt;

fn main() {
    // Initialise any tracing subscriber — fmt is the simplest.
    fmt::init();

    let engine = Engine::new("", Duration::from_secs(60));
    let mut ctx = Context::new();
    ctx.set("who".to_string(), "world".to_string());

    // `staticweaver.render_template` span with template_len field
    // appears in your subscriber output.
    let _ = engine.render_template("hello {{who}}", &ctx).unwrap();
}
```

Spans: `staticweaver.render_template` (fields: `template.bytes`) and `staticweaver.render_page` (fields: `layout`, `context.len`). Zero weight when the `tracing` feature is off.

</details>

<details>
<summary><b>Cache stats for prometheus / metrics / opentelemetry (v0.0.4)</b></summary>

```rust
use staticweaver::cache::{Cache, CacheStats};
use std::time::Duration;

fn main() {
    let mut cache: Cache<String, u32> = Cache::new(Duration::from_secs(60));
    let _ = cache.insert("a".to_string(), 1);
    let _ = cache.get(&"a".to_string());         // hit
    let _ = cache.get(&"missing".to_string());   // miss

    let s: CacheStats = cache.stats();
    println!(
        "inserts={} hits={} misses={} evictions={} ttl_expired={}",
        s.inserts, s.hits, s.misses, s.evictions, s.ttl_expired,
    );
    assert_eq!(s.hits, 1);
    assert_eq!(s.misses, 1);
}
```

All counters are monotonically increasing `u64`s. Take two snapshots and subtract for per-window rates.

</details>

<details>
<summary><b>CLI binary</b></summary>

```bash
cargo install staticweaver

staticweaver render hello.html --set name=Ada
echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
```

`<template>` is a file path or `-` for stdin. `--set KEY=VALUE` is repeatable. `--no-escape` disables HTML escape. Errors exit non-zero.

</details>

---

## Configuration

<details>
<summary><b>Engine construction</b></summary>

```rust
use staticweaver::Engine;
use std::time::Duration;

let mut engine = Engine::new("templates", Duration::from_secs(3600));

let plain = Engine::new("templates", Duration::from_secs(3600))
    .with_html_escape(false);

engine.set_delimiters("<%", "%>");
engine.set_max_cache_size(1024);
engine.clear_cache();
```

</details>

<details>
<summary><b>Cache construction</b></summary>

```rust
use staticweaver::cache::Cache;
use std::time::Duration;

let a: Cache<String, String> = Cache::new(Duration::from_secs(60));

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

All examples live in `examples/` and use the shared `support.rs` helper for the spinner/checkmark UI. See [`examples/README.md`](examples/README.md) for an annotated index.

| Example | Covers |
| :--- | :--- |
| `hello` | Getting started: build an `Engine`, populate a `Context`, render a template |
| `context` | Insert, update, remove, iterate; typed values; dot-notation; hash stability |
| `cache` | TTL expiration, **LRU eviction**, refresh, update, `IntoIterator` |
| `engine` | Escaping defaults, `{{!key}}` opt-out, partials, filters, control flow, dot-notation, `render_page` with subdirectories, custom delimiters, path-traversal rejection |
| `errors` | Every `EngineError` / `TemplateError` variant and its conversions |
| `inheritance` | `{{#extends}}` + `{{#block}}` + `{{ super() }}` showcase. Layered base/child layouts. |
| `filters` | The 23 built-in filters plus a custom `slugify` filter and `is vip` test, registered via `add_filter` / `add_test`. |
| `loaders` | `FsLoader` (default), `MemoryLoader` (in-memory), and a custom hot-mutable `LiveLoader` implementing the `TemplateLoader` trait. |
| `control_flow` | Expression language, `{{#each list}}`, `{{#each 1..N}}`, `{{#break}}`, `{{#continue}}`, `{{#set}}`. |
| `remote` | (feature-gated) `create_template_folder(Some(url))` against a local mock server. Run with `cargo run --example remote --features remote-templates`. |
| `async_tokio` | (feature-gated) **v0.0.4** Async render via Tokio: `MemoryAsyncLoader`, `render_template_async`, `render_page_async`, `render_to_async` (AsyncWrite sink), and `Arc<Engine>` fan-out across tokio tasks. Run with `cargo run --example async_tokio --features async-tokio`. |
| `axum` | (feature-gated) End-to-end Axum web-server integration: render-to-`String`, render-to-`Vec<u8>` via `render_to`, per-request context. Run with `cargo run --example axum --features axum-example`. |

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

### Property-based + differential testing

```bash
cargo test --test proptest_parser    # 6 properties × 256 cases each = ~1500 inputs
cargo test --test differential       # 9 byte-equality assertions vs minijinja
cargo test --test snapshots          # 16 golden-output regressions
```

### Documentation

| Surface | Hosted at | Built by | Trigger |
| :--- | :--- | :--- | :--- |
| API reference (release) | [docs.rs/staticweaver](https://docs.rs/staticweaver) | docs.rs | `crates.io` publish |
| API reference (tip-of-`main`) | [doc.staticweaver.com](https://doc.staticweaver.com/) | `.github/workflows/docs.yml` → GitHub Pages (Pages-via-Actions, no `gh-pages` branch) | every push to `main` |
| README + crate-level prose | [docs.rs/staticweaver](https://docs.rs/staticweaver) (front page) | `#![doc = include_str!("../README.md")]` in `src/lib.rs` | every `cargo doc` |
| CHANGELOG / SECURITY / FAQ | This GitHub repo | -- | every push |

Both the CI `Docs` job and the standalone `docs.yml` workflow build
documentation under `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links"`,
run every doctest with `--all-features`, and enforce **100 % example coverage**.
Missing-docs warnings are upgraded to **`deny`** at the crate level
(`Cargo.toml [lints.rust]`), so any undocumented public item fails
`cargo build` directly — not just CI.

Publication targets:
- **Release docs** are built by docs.rs on every `cargo publish`.
- **Tip-of-`main` docs** are built by `.github/workflows/docs.yml` and
  deployed via `actions/deploy-pages` (GH-Pages-via-Actions). No
  `gh-pages` branch exists; the artifact is uploaded directly and the
  custom-domain `CNAME` is written on every deploy.

### CI

| Workflow | Trigger | Purpose |
| :--- | :--- | :--- |
| `ci.yml` | push, PR | Clippy, fmt, test (Linux + macOS + Windows), coverage gate (98%), `cargo deny`, via reusable pipelines |
| `docs.yml` | push to `main` | Build rustdoc + publish to GitHub Pages at `doc.staticweaver.com` |
| `release.yml` | tag `v*.*.*` | Validate matrix (macOS + Linux + Windows), build artifacts, GitHub Release, crates.io publish |

See [CONTRIBUTING.md](CONTRIBUTING.md) for signed commits and PR guidelines.

---

## Security

<details>
<summary><b>Safety guarantees</b></summary>

- `#![forbid(unsafe_code)]` across the entire codebase
- HTML escape on by default for `render_template` / `render_page` substitutions (5-character OWASP set: `& < > " '`)
- `render_page` layout names validated before any filesystem call (rejects `..`, `/`, `\`, null bytes)
- Remote template fetching gated behind an opt-in `remote-templates` cargo feature; default build has no networking
- 10 s timeout and 1 MiB body cap on every remote fetch
- `Content-Type` validation on remote fetches — rejects non-textual MIME types
- No default third-party URL — `create_template_folder(None)` is an error, not a silent download
- Property-based fuzzing via proptest — ~1500 random inputs per `cargo test`, never panic
- Differential testing vs Minijinja — byte-equality on shared syntax
- `cargo audit` and `cargo deny` clean (advisories, bans, licenses, sources)
- Yanked crates denied via `[advisories] yanked = "deny"`
- All commits GPG-signed; `Assisted-by:` trailer per the Linux kernel coding-assistants convention
- SPDX license headers on all source files

</details>

---

## FAQ

### Choosing the right tool

<details>
<summary><b>When should I pick staticweaver vs Tera, Minijinja, Handlebars, or Askama?</b></summary>

| You need… | Pick |
| :--- | :--- |
| Compile-time-checked templates with zero runtime parsing | **Askama** (different ergonomics — templates become Rust types) |
| Rich Jinja2 feature set (macros, custom filters w/ Tera args, i18n hooks) | **Tera** |
| Bytecode VM and the absolute fastest runtime engine for complex templates | **Minijinja** |
| Mustache compatibility for porting templates between languages | **Handlebars** or **staticweaver** |
| **Small (`#![forbid(unsafe_code)]`), Tera-style expressions + inheritance with `super()`, SIMD HTML escape, MSRV 1.68, networking is opt-in** | **staticweaver** |

The realistic differentiator: staticweaver is the only Rust template engine that combines `#![forbid(unsafe_code)]` with full inheritance + `super()`, range iteration, custom filters/tests, pluggable loaders, and SIMD escape — and stays small enough (~5k LoC engine) to read in an afternoon.

</details>

<details>
<summary><b>Is it production-ready? What's the stability story?</b></summary>

`v0.0.3` is the latest release on crates.io. It builds on the `v0.0.2` cycle (which moved the engine beyond Mustache-tier substitution) with HTML-escape idempotency, opt-in lax mode, a collision-safe `Context::hash()`, a re-tuned escape fast path, and the full Dependabot backlog drained (including RUSTSEC-2026-0185 remediation). It's tested with **480+ tests** (lib, integration, snapshot, differential vs Minijinja, property-based via proptest, lax-mode matrix), **98% line-coverage floor enforced in CI**, **100% rustdoc example coverage compile-time-enforced** (`missing_docs = "deny"`), and a **comparative bench matrix** vs Tera/Minijinja/Askama. Cross-platform CI runs on Linux, macOS, and Windows. `#![forbid(unsafe_code)]` is enforced at the crate root.

That said — it's still pre-1.0, so the API may change before v1. We document every breaking change in [`CHANGELOG.md`](CHANGELOG.md). Pin a precise version in your `Cargo.toml` (`staticweaver = "=0.0.4"`) if you want to control upgrades manually.

</details>

### Using the library

<details>
<summary><b>Why does <code>render_page</code> error on missing keys but <code>{{#if x}}</code> doesn't?</b></summary>

Two different contracts:

- **Substitution** (`{{x}}`) is strict — a missing key is a programming error and the engine refuses to silently render the empty string.
- **Conditionals** (`{{#if x}}`) are lenient — missing keys evaluate to `Value::Null`, which is falsy. This matches Jinja/Tera and lets you write `{{#if optional_thing}}…{{/if}}` without pre-checking.

Distinguish "missing" from "explicitly `Null`" via `{{#if x is defined}}` (key exists, even if value is `Null`) vs `{{#if x is none}}` (value is exactly `Null`).

</details>

<details>
<summary><b>How do I escape <code>{{</code> in the output?</b></summary>

Three ways:

1. **Backslash-escape**: `\{{literal}}` emits `{{literal}}` verbatim.
2. **Custom delimiters**: `engine.set_delimiters("<%", "%>")` swaps `{{` for any pair you like.
3. **Substitute the literal**: `{{open}}` with `ctx.set("open", "{{".to_string())`.

</details>

<details>
<summary><b>How do I disable HTML escaping for one tag, vs globally, vs per file extension?</b></summary>

| Scope | How |
| :--- | :--- |
| **Per-tag** (raw output) | `{{!key}}` or `{{ key \| safe }}` |
| **Globally** (non-HTML output) | `Engine::new(...).with_html_escape(false)` |
| **Per file extension** (HTML pages escape, `.txt` doesn't) | `engine.autoescape_on(&[".html", ".xml"])` |

The default escapes the 5-character OWASP set. `/` is *not* escaped — Minijinja escapes `/` defensively, we don't (matches Askama).

</details>

<details>
<summary><b>How do I share an engine across threads / async tasks?</b></summary>

Wrap it in `Arc`. The engine is `Send + Sync` and `render_template` is `&self`:

```rust,ignore
use std::sync::Arc;
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Arc::new(Engine::new("templates", Duration::from_secs(60)));
let mut ctx = Context::new();
ctx.set("name".to_string(), "Ada".to_string());

let e = engine.clone();
tokio::spawn(async move {
    let _ = e.render_template("hello {{name}}", &ctx);
});
```

`render_page` is `&mut self` because of the page cache, so for shared use either wrap in a `Mutex<Engine>` or call `render_template` directly with the layout body you've loaded yourself.

</details>

<details>
<summary><b>Can I use it with Axum, Actix, Rocket, or Warp?</b></summary>

Yes — render to a `String` (`render_template` / `render_page`) or stream into the response body (`render_to` / `render_page_to` accepts any `io::Write`). There is no framework integration layer because none is needed; the engine is a pure function over `(template, context) -> String`.

A working Axum example with three integration patterns lives in [`examples/axum.rs`](examples/axum.rs). Run it with `cargo run --example axum --features axum-example`.

</details>

<details>
<summary><b>Is staticweaver async?</b></summary>

No. The render path is synchronous and CPU-bound — there is no I/O on the hot path. `render_to` works against an `io::Write` (sync) sink.

The opt-in remote-template downloader uses blocking `reqwest`. If you need to fetch templates from inside an async task, call `create_template_folder` from `tokio::task::spawn_blocking`.

Why no async render? The workload is parser + tree-walk, not network. Forcing an async runtime onto callers who don't need one is a net loss in dependency weight and complexity.

</details>

<details>
<summary><b>How do I add a custom filter or test?</b></summary>

`Engine::add_filter` for filters (`{{ x | name:arg }}`); `Engine::add_test` for tests (`{{#if x is name}}`). Both take `Arc<Fn>` closures and override built-ins of the same name:

```rust
use staticweaver::{Context, Engine};
use staticweaver::context::Value;
use std::sync::Arc;
use std::time::Duration;

let mut engine = Engine::new("", Duration::from_secs(60));

engine.add_filter(
    "slugify",
    Arc::new(|input, _args| Ok(input.to_lowercase().replace(' ', "-"))),
);

engine.add_test(
    "admin",
    Arc::new(|v: &Value, _args| {
        Ok(matches!(v, Value::String(s) if s == "admin"))
    }),
);
```

See [`examples/filters.rs`](examples/filters.rs) for a runnable showcase.

</details>

<details>
<summary><b>How does the cache work and when does it invalidate?</b></summary>

Only `render_page` caches. The cache key is `"{layout}:{Context::hash()}"` — `Context::hash()` is order-independent so two contexts with the same logical contents always hit the same entry.

Eviction is **true LRU**: when the cache reaches its capacity bound, the least-recently-used entry is evicted on the next insert. `Cache::get` bumps access recency. TTL expiration is also enforced.

`render_template` is uncached — pure function, no state. To invalidate: `engine.clear_cache()`. See [`PERFORMANCE.md`](PERFORMANCE.md) for hit-vs-miss benchmark numbers (~6.7× faster on a hit).

</details>

### Errors and debugging

<details>
<summary><b>How do I get useful error messages?</b></summary>

Every user-facing error from `render_template` / `render_page` carries `at line N, column M`:

```text
Render error: Unresolved template tag: missing at line 2, column 9
Invalid template: Unclosed template tag at line 5, column 12
Render error: #each: unresolved list `posts` at line 7, column 14
```

The position points at the offending byte in the *original* template — for partials and `{{#extends}}` chains, the position refers to the included file, not the parent template.

</details>

<details>
<summary><b>The engine panicked on weird input — is that a bug?</b></summary>

Yes. The engine is fuzzed via [proptest](tests/proptest_parser.rs) — 6 properties × 256 cases each (~1500 random inputs per `cargo test`) — asserting that arbitrary text, malformed tags, random expressions, and edge-case math (full `i64` range × all four ops) **never panic**.

If you find input that panics: please file an issue with the template + context. We'll add it to the proptest harness so it can never regress.

</details>

<details>
<summary><b>I want to test my templates without writing a Rust harness — how?</b></summary>

`cargo install staticweaver` ships a CLI binary:

```bash
staticweaver render hello.html --set name=Ada --set greeting=Hi
echo 'Hi {{name}}!' | staticweaver render - --set name=Ada
```

`<template>` is a file path or `-` for stdin. `--set KEY=VALUE` is repeatable. `--no-escape` disables HTML escape.

</details>

### Versioning + maintenance

<details>
<summary><b>What's the MSRV and what controls it?</b></summary>

**MSRV is 1.68.** The floor is set by `thiserror 2.0`, `regex 1.12`, and `serde_json 1.0.149` — not by anything we use directly. Bumping MSRV is a breaking change, so we only do it when an upstream dep forces our hand.

Stable Rust only — we don't use any nightly features.

</details>

<details>
<summary><b>Does it pull in OpenSSL / heavy networking deps?</b></summary>

**No, by default.** The default build has zero networking. Five direct runtime deps: `fnv`, `askama_escape`, `tempfile`, `thiserror`, and that's it.

Networking is gated behind the `remote-templates` feature. Even when enabled, `reqwest` uses `rustls-native-certs` (no OpenSSL pull-in). The `json` and `axum-example` features each gate their own deps.

</details>

<details>
<summary><b>Can I depend on it from a <code>no_std</code> project?</b></summary>

Not currently — we use `std::collections::HashMap`, `std::fs`, `std::io::Write`. Adding `no_std` support would require the engine to operate over `alloc` collections and stub the filesystem-backed `FsLoader`. The `std::time::Instant` dependency in the cache is the structural blocker (no `core` equivalent). If you have a concrete use case, file an issue.

</details>

---

## License

Dual-licensed under [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT), at your option.

See [CHANGELOG.md](CHANGELOG.md) for release history, [SECURITY.md](SECURITY.md) for the disclosure policy, and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community guidelines.

<p align="right"><a href="#contents">Back to Top</a></p>
