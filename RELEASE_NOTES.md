# `staticweaver` v0.0.2

`v0.0.2` graduates `staticweaver` from a Mustache-tier substituter into a **full
Tera-tier templating engine** while keeping the project small, safe, and
dependency-light. **89 commits, +18.3k lines, 460+ tests, 99.16% line coverage,
18/18 CI checks green.**

The headline shift: it now ships full inheritance with `{{ super() }}`, a
recursive-descent expression language, 23 built-in filters, custom
filters/tests, pluggable template loaders, SIMD HTML escape that matches Askama
on long inputs, line:column error messages, stream rendering, a CLI binary —
and stays at `#![forbid(unsafe_code)]` with five direct runtime dependencies.

## Install

```toml
[dependencies]
staticweaver = "0.0.2"
```

```bash
# Optional CLI binary
cargo install staticweaver
```

## What's new

### Templating language

- **Expression language inside `{{#if EXPR}}`** — comparisons (`==` `!=` `<`
  `<=` `>` `>=`), short-circuiting boolean operators (`and` `or` `not`),
  checked integer math (`+` `-` `*` `/`), string concat (`~`), postfix tests
  (`is defined`, `is empty`, `is none`, with `is not` for negation).
- **Template inheritance** — `{{#extends "base"}}` plus
  `{{#block "name"}}…{{/block}}` with multi-level chains. Children include the
  parent block body via `{{ super() }}`.
- **`{{#each}}` everywhere** — over `List` and `Map` values, with
  `@index`/`@first`/`@last`/`@key` loop helpers. Range form
  (`{{#each 1..N}}`). Loop control via `{{#break}}` and `{{#continue}}`.
- **23 built-in filters** — case folding, trimming, slicing, padding, number
  formatting, truthy tests, URL encoding, `safe`. Optional `json` filter under
  `--features json`.
- **Custom filters and tests at runtime** — `Engine::add_filter` and
  `Engine::add_test` take `Arc<Fn>` closures and override built-ins of the
  same name.
- **Whitespace control** (`{{- key -}}`), **comments**
  (`{{!-- multi-line --}}`), **backslash-escape delimiters**
  (`\{{literal}}`), **custom delimiters**, **dot-notation lookups**
  (`{{user.email}}`).

### Engine APIs

- **Polymorphic `Value` enum** (`Null`/`Bool`/`Number`/`String`/`List`/`Map`)
  replaces the v0.0.1 flat `String → String` map. `Context::set_value` accepts
  `Into<Value>` for `String`, `&str`, `bool`, `i32`, `i64`, `Vec<V>`.
- **Stream rendering** —
  `Engine::render_to<W: io::Write>(template, ctx, &mut writer)` and
  `render_page_to` write directly into any `io::Write` sink without an
  intermediate `String`.
- **Pluggable template loaders** — implement `TemplateLoader` for any backend
  (memory, database, embedded asset bundle). Built-in `FsLoader` and
  `MemoryLoader`. Plug in via `Engine::with_loader(Arc<dyn …>, ttl)`.
- **Per-extension auto-escape** — `engine.autoescape_on(&[".html", ".xml"])`
  makes `render_page` auto-escape only for matching extensions. Tera-style.
- **Line:column in every error message** — `Render error: Unresolved template
  tag: missing at line 2, column 9`. Pointer-arithmetic on slices into the
  original template; works across partial / extends boundaries.
- **CLI binary** — `cargo install staticweaver` ships a `staticweaver`
  executable. `staticweaver render <template> [--set k=v ...] [--no-escape]`.

### Performance

- **SIMD HTML escape via `askama_escape`** — matches Askama on `escape_heavy`
  (23.3 µs vs 23.2 µs), beats Tera 3.3× on the same workload.
- **`#each` clone hoisted out of loop body** — `each_1000` 22.6 ms → 557 µs
  (~40× faster).
- **True LRU cache eviction** with TTL — `render_page` cache hits are 6.7×
  faster than misses.
- **Comparative benchmark matrix** vs Tera, Minijinja, Askama. Wins or ties
  Minijinja on 4/7 workloads. Reproduce: `cargo bench --bench comparative`.

| Workload | staticweaver | Tera | Minijinja | Askama |
| :--- | ---: | ---: | ---: | ---: |
| `simple_sub` (1 tag) | **497 ns** | 388 ns | 591 ns | 95 ns |
| `many_sub_32` | **12.85 µs** | 5.96 µs | 14.40 µs | 973 ns |
| `escape_heavy` (10 KB body) | **23.3 µs** | 77.8 µs | 24.3 µs | 23.2 µs |
| `each_100` | 58.3 µs | 17.8 µs | 23.6 µs | 5.24 µs |
| `each_1000` | 557 µs | 171 µs | 184 µs | 51.9 µs |
| `if_chain` | 2.51 µs | 455 ns | 656 ns | 25.4 ns |
| `filter_chain` | **1.03 µs** | 620 ns | 988 ns | 198 ns |

Full numbers + reproduction in [`PERFORMANCE.md`](PERFORMANCE.md).

### Robustness

- **Property-based testing** (proptest) — 6 properties × 256 cases ≈ 1500
  random inputs per `cargo test` run, asserting the engine never panics on
  arbitrary input.
- **Differential testing vs Minijinja** — 9 byte-equality assertions on shared
  syntax (substitution, escape, if/else, each, filters, dot-paths).
- **Snapshot tests** — 16 golden-output regression guards.
- **Cross-platform CI** — Linux + macOS + Windows on every PR.

### Documentation

- **100% / 100% rustdoc coverage** — every public item documented, every
  public item carries a runnable example.
- **98 doctests** across `src/` and `README.md`.
- **11 runnable examples** with a shared spinner/checkmark UI: `hello`,
  `context`, `cache`, `engine`, `errors`, `inheritance`, `filters`, `loaders`,
  `control_flow`, `remote`, `axum`.
- **14-question FAQ** organised under "Choosing the right tool", "Using the
  library", "Errors and debugging", "Versioning + maintenance".

## Breaking changes

These are flagged so downstream consumers know what to update. None of them
should be unexpected — every breaking change documents the v0.0.1 → v0.0.2
migration path.

| Change | Migration |
| :--- | :--- |
| `Context::iter()` yields `(&String, &Value)` (was `(&String, &String)`) | Pattern-match on `Value` or use `value.as_str()` |
| `Deref / DerefMut<Target = FnvHashMap<String, String>>` removed from `Context` | Use `set_value` / `get_value` / `get_path` / `set_value_str` / `set_value_string` |
| `Context::get`, `get_mut`, `remove` only return `Option<&String>` for `Value::String` entries | Use `get_value` for the typed `Value` |
| `TemplateError::EngineError(Box<EngineError>)` removed | Catch `EngineError::Template(TemplateError)` instead |
| `PageOptions` removed (dead code) | Use `Context` directly |
| `Cache::get` is `&mut self` (was `&self`); `Cache<K, V>` adds `K: Clone` | Update method receivers; constraint already met by `String` |
| HTTP fetching moved behind `remote-templates` cargo feature | Add `features = ["remote-templates"]` if you used the downloader |
| Default-URL fallback in `create_template_folder(None)` removed | Pass an explicit URL or use `create_template_folder_with_files` |

## Acknowledgements

This release was developed alongside multi-agent code review and architectural
analysis sessions; every commit is GPG-signed and carries an `Assisted-by:`
trailer per the Linux kernel coding-assistants convention.

The SIMD HTML escape path uses [`askama_escape`](https://crates.io/crates/askama_escape)
— the same encoder that powers Askama's compile-time auto-escape.

Differential and proptest harnesses cross-validate against
[`minijinja`](https://crates.io/crates/minijinja),
[`tera`](https://crates.io/crates/tera), and
[`askama`](https://crates.io/crates/askama).

## Links

- [Crate](https://crates.io/crates/staticweaver) ·
  [Documentation](https://docs.rs/staticweaver) ·
  [Repository](https://github.com/sebastienrousseau/staticweaver)
- [README](README.md) — full feature reference, FAQ, library usage
- [CHANGELOG](CHANGELOG.md) — granular release history
- [PERFORMANCE](PERFORMANCE.md) — bench progression, caching model, methodology
- [SECURITY](SECURITY.md) — disclosure policy
- [CONTRIBUTING](CONTRIBUTING.md) — contributor guide

---
THE ARCHITECT ᛫ Sebastien Rousseau ᛫ https://sebastienrousseau.com
THE ENGINE ᛞ EUXIS ᛫ Enterprise Unified Execution Intelligence System ᛫ https://euxis.co
