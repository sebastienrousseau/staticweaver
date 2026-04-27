# Changelog

All notable changes to `staticweaver` are documented in this file. See
[README.md](README.md) for an overview of the engine's capabilities, the
[Templating Language](README.md#templating-language) reference, and the
public API.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-`1.0.0`, breaking changes may occur in minor/patch releases and are
called out explicitly below).

## [0.0.2] - 2026-04-26

The `v0.0.2` cycle moved staticweaver from a Mustache-tier substituter
into a Tera-tier templating engine: control flow, a small expression
language, partials, template inheritance, and a built-in filter
pipeline — while keeping the project small, safe, and dependency-light.
**220 lib tests** (was 12), **97% line coverage** enforced in CI,
cross-platform matrix on Linux + macOS + Windows.

### Added — engine features

- **Polymorphic `Value` enum** — `Context` now stores
  `FnvHashMap<String, Value>` with `Null`, `Bool`, `Number(i64)`,
  `String`, `List`, and `Map` variants. Adds `set_value`, `get_value`,
  `get_path` for typed and dot-notation access. `Value` implements
  `Display`, `From<String/&str/bool/i32/i64/Vec<V>>`. Backwards-
  compatible `set(String, String)` still wraps as `Value::String`.
- **Dot-notation lookup** — `{{user.name}}` walks `Value::Map`;
  `{{items.0}}` indexes `Value::List` by position.
- **Control-flow blocks** — `{{#if EXPR}}…{{else}}…{{/if}}` and
  `{{#each list}}…{{/each}}`. `#each` exposes `@index`, `@first`,
  `@last`, and `@key` (for `Map` iteration), binding each element to
  `this`. Block bodies render through the same parser, so escaping,
  dot-notation, filters, and nested blocks compose naturally.
- **Expression language** inside `#if`:
  - Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`. Equality works
    across types via structural equality; ordered comparisons require
    Number-Number or String-String.
  - Boolean operators: `and`, `or`, `not` with conventional precedence
    (NOT > AND > OR) and short-circuit evaluation.
  - Integer math: `+`, `-`, `*`, `/` with checked arithmetic;
    overflow and division-by-zero return `InvalidTemplate` rather than
    panicking. Multiplicative operators bind tighter than additive.
  - Postfix tests: `is defined`, `is empty`, `is none`, with
    `is not` for negation. `defined` checks key presence on bare-path
    operands; `empty` reports true for empty `String`/`List`/`Map` and
    `Null`; `none` is strictly `Value::Null`.
  - Bare paths like `{{#if user}}` keep their truthiness semantics
    (backwards-compatible).
- **Partials** — `{{> name}}` reads `name.html` from the template
  root and substitutes the parent context. Pass scoped parameters via
  `{{> name k=v}}`. Recursion capped at depth 10.
- **Template inheritance** — `{{#extends "base"}}` plus
  `{{#block "name"}}…{{/block}}` lets a child template override named
  blocks in its parent. Multi-level chains compose; the child wins on
  conflicting block names.
- **In-template assignment** — `{{#set name = LITERAL}}` binds a
  value locally for subsequent tags. Local-scope only — does not leak
  into the parent context.
- **Filter pipeline** — `{{ x | filter | filter:arg }}` with a
  quoted-CSV argument parser. Built-in filters: `uppercase`,
  `lowercase`, `trim`, `truncate`, `capitalize`, `length`, `default`,
  `replace`, `urlencode`, `safe`.
- **Comments** — `{{! one-line }}` and `{{!-- multi-line --}}`,
  stripped before rendering.
- **Whitespace control** — `{{- key -}}` trims adjacent whitespace
  on the corresponding side of the tag.
- **Backslash escape** — `\{{literal}}` emits the delimiter as
  literal text. Even-length backslash runs collapse to literal
  backslashes; odd-length runs escape the following delimiter.
- **HTML escape by default** — `Engine::render_template` /
  `render_page` escape `& < > " '`. Per-tag opt-out: `{{!body}}`.
  Global opt-out: `Engine::new(...).with_html_escape(false)`.
- **Layout-name validation in `render_page`** — rejects `/`, `\`,
  `..`, and null bytes before touching the filesystem.
- **`with_html_escape(bool)`** builder method on `Engine`.
- **Whitespace trimming** around tag keys — `{{ name }}` and
  `{{name}}` are equivalent.
- **Configurable downloader file list** — `DEFAULT_TEMPLATE_FILES`
  exposed as a public constant; new
  `Engine::create_template_folder_with_files(path, &[…])` lets
  callers override the historical six-filename set.
- **Stray closing tags** or `{{else}}` outside a block produce a
  clear `InvalidTemplate` error.
- **String concat operator `~`** in expressions —
  `{{#if name ~ " Lovelace" == "Ada Lovelace"}}…{{/if}}`. Lower
  precedence than math, higher than comparisons. Tera/Twig style.
  All operands coerce via `Display` (Number → `"5"`, Null → `""`).
- **Loop control: `{{#break}}` / `{{#continue}}`** inside `#each`.
  Bubble through nested `#if` / `#block` until the enclosing
  `#each` catches them. Partials and the top-level renderer
  swallow the signal so they remain self-contained.
- **Range iteration in `#each`** — `{{#each START..END}}` (END
  exclusive). Both bounds are full expressions, so paths and
  arithmetic both work: `{{#each 0..items.length}}` and
  `{{#each 0..n + 1}}`. Loop helpers (`@index`, etc.) bind the
  same way as for List/Map iteration.
- **`{{ super() }}` in inherited blocks** — child overrides can
  include the parent block's body. Renders through the full
  pipeline (escape, filters, dot-paths). Outside an override
  context, `super()` is a silent no-op. Does not leak across
  partial boundaries.
- **Custom filters API** — `Engine::add_filter(name, FilterFn)`
  registers a `Fn(&str, &[String]) -> Result<String, EngineError>`
  closure. Custom filters override built-ins of the same name.
- **Custom tests API** — `Engine::add_test(name, TestFn)`
  registers an `Fn(&Value, &[String]) -> Result<bool, EngineError>`
  closure. Custom tests override built-in `defined`/`empty`/`none`
  of the same name.
- **`TemplateLoader` trait** for pluggable template backends.
  Built-in `FsLoader` (default) and `MemoryLoader` (for tests /
  embedded assets). New `Engine::with_loader(Arc<dyn …>, ttl)`
  constructor lets callers plug in custom backends.
- **Per-extension auto-escape policy** — `engine.autoescape_on(
  &[".html", ".xml"])` makes `render_page` auto-escape only
  layouts whose name ends with one of the listed suffixes.
  Matches Tera's behaviour. `render_template` (no layout name)
  is unaffected.
- **Stream rendering** — `render_to<W: io::Write>(template, ctx,
  &mut writer)` and `render_page_to<W: io::Write>(ctx, layout,
  &mut writer)` write directly into any `io::Write` sink. Saves
  the `String → Vec<u8>` conversion in HTTP / file workflows.
- **Line:column in error messages** — every user-facing error from
  `render_template` / `render_page` now carries
  `at line N, column M`. Pointer-arithmetic on slices into the
  original template; works for the main template, partials, and
  inherited base files.
- **15 new built-in filters**: `abs`, `round`, `ceil`, `floor`,
  `number_format` (configurable thousands separator), `repeat`,
  `reverse`, `slice` (Unicode-aware), `pad_start`, `pad_end`,
  `contains`, `starts_with`, `ends_with`. Plus `json` (under
  `--features json`, pulls `serde_json`) for `{{ data | json |
  safe }}`-style state embedding into HTML/JS.
- **CLI binary** — `cargo install staticweaver` now produces a
  `staticweaver` executable. Hand-rolled arg parsing (no `clap`
  dep). Usage: `staticweaver render <template> [--set k=v ...]
  [--no-escape]`. Reads templates from a file path or stdin
  (`-`).

### Added — tooling, governance, and CI

- **`remote-templates` cargo feature** — fetching templates via
  HTTP/S is now opt-in; default build has no networking code.
- **Bounded HTTP downloads** — 1 MiB per-file cap enforced against
  both `Content-Length` and the actual read.
- **`Content-Type` validation** on remote template fetches — rejects
  responses whose MIME type does not look textual (non-`text/*`,
  non-JavaScript, non-JSON, non-XHTML).
- **`#[cfg_attr(docsrs, doc(cfg(feature = "remote-templates")))]`**
  on every feature-gated item so docs.rs renders the
  "available on crate feature `remote-templates` only" badge.
- **100% doc coverage** with examples across every public item;
  doctests exercised in CI under
  `-D rustdoc::broken_intra_doc_links`.
- **Cross-platform CI** — `run-cross-platform: true` in `ci.yml`
  fans every PR to macOS + Windows runners. Multi-OS `verify` job
  in `release.yml`.
- **Coverage gate** — `coverage-gate` CI job fails the build if
  line coverage drops below **98%**. Achieved 98.6% lines /
  98.4% functions / 98.2% regions across all source files.
  `make coverage` produces the same report locally.
- **Property-based robustness tests** —
  `tests/proptest_parser.rs` runs 256 random cases across 6
  properties (~1500 inputs per `cargo test`) asserting the
  engine never panics on arbitrary input — only returns clean
  `EngineError`s.
- **Differential tests vs Minijinja** —
  `tests/differential.rs` renders the same logical
  template+context through both engines and asserts byte-for-byte
  identical output across substitution, escape, if/else, each,
  filters, and dot-path lookups.
- **CLI smoke tests** — `tests/cli_smoke.rs` spawns the
  `staticweaver` binary and exercises every flag (file path,
  stdin, `--set`, `--no-escape`, `--help`, `--version`) plus
  the error paths (missing template, malformed `--set`, render
  errors, unknown subcommand). 10 tests.
- **Axum integration example** — `examples/axum.rs` (gated
  behind `axum-example`) boots a minimal HTTP server
  demonstrating render-to-`Html<String>`, render-to-`Vec<u8>`
  via `render_to`, and per-request context from path
  parameters.
- **Mock-server integration tests** — 6 new tests in
  `tests/download_tests.rs` covering the remote-templates HTTP path
  (happy path, 404, bad `Content-Type`, oversized `Content-Length`,
  JavaScript MIME acceptance, missing Content-Type tolerance). Uses
  `mockito` as a dev-dep.
- **Portable git hooks** — repo-local
  `.githooks/{pre-commit,commit-msg,pre-push}` (POSIX `sh`).
  Installed by `make init`; enforces `commit.gpgsign=true`,
  Conventional-Commits subjects, and runs the full test battery
  before `git push`.
- **`rust-toolchain.toml`** pinned to `stable` with
  `rustfmt` + `clippy`.
- **Shared `examples/support.rs`** spinner/checkmark helpers; all
  examples renamed to one-word filenames (`hello`, `context`,
  `cache`, `engine`, `errors`).
- **`examples/remote.rs`** — feature-gated example demonstrating
  `create_template_folder(Some(url))` against a local `mockito`
  server.
- **`examples/README.md`** — annotated index of all six examples.
- **`CHANGELOG.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`** added.
- **`.github/labeler.yml`** for automatic PR triage by path glob.
- **Docs CI job** — `cargo doc` under strict flags + doctest run
  + 100% example coverage gate.

### Performance

- `Context::hash` rewritten to a commutative XOR-combiner — O(n),
  zero allocation. `context_hash_100_keys` bench: 9.68 µs → 4.86 µs
  (−50%).
- `escape_html_into` rewritten to byte-scan with run flushing.
  `render_template_escape_heavy` bench (10 KiB, 5% metachars):
  41.82 µs → 35.22 µs (−16%). Single-tag baseline: 226 ns → 214 ns
  (−5%).
- Three new criterion benches
  (`render_template_escape_heavy`, `context_hash_100_keys`,
  `render_template_32_tags`) guard these gains against regression.
- **LRU cache eviction** — capacity-pressure inserts now evict the
  least-recently-used entry instead of clearing the whole cache.
- **Phase D — closing the gap on Tera and Minijinja**:
  - **Comparative bench matrix** vs Tera, Minijinja, Askama in
    `benches/comparative.rs` (7 workloads × 4 engines, Criterion
    groups).
  - **SIMD HTML escape** via `askama_escape::Html` — same five-char
    contract as before, ~10× faster on long inputs.
    `escape_heavy/sw`: 34.4 µs → 22.8 µs (−34%, now matches Askama
    at 22.9 µs and beats Tera at 84.2 µs by 3.7×).
  - **Hoisted context clone out of `#each` loop** — was cloning the
    full `Context` per iteration. `each_1000/sw`: 22.6 ms → 640 µs
    (35×).
  - **`Context::set_value_str(&str, V)`** — borrowed-key counterpart
    to `set_value` that reuses the existing slot on update,
    eliminating per-iteration `String` allocs in the loop helpers
    (`this`, `@index`, `@first`, `@last`, `@key`).
    `each_1000/sw`: 640 µs → 563 µs (−12%).
  - **`Context::set_value_string(&str, &str)`** — `Value::String`
    fast path that reuses the destination buffer in place via
    `clear()` + `push_str()` instead of allocating a new `String`.
    Wired into the `#each` iterator for `Value::String` items.
    `each_100/sw`: 67 µs → 55 µs (−18%).
  - **Allocation-free close-tag match in `extract_block`** — was
    allocating a `String` via `format!("/{block}")` on every nested
    tag scan; replaced with `strip_prefix('/')` + equality.
  - **Cumulative each_1000 win: 22.6 ms → 535 µs (42×)**;
    each_100: 326 µs → 54.9 µs (5.9×); escape_heavy: 34.4 µs →
    22.8 µs (1.5×).
  - **Final positioning** (full-quality 5 s measurements):
    wins/ties Minijinja on `simple_sub`, `escape_heavy`,
    `many_sub_32`, `filter_chain` (4 / 7); ties Askama on
    `escape_heavy`; beats Tera on `escape_heavy` 3.7×. Remaining
    2.85–3.6× gap on loops/conditionals is constant-factor per-tag
    overhead in the AST walker; closing it would require a bytecode
    compiler (explicitly out of scope).
  - **`PERFORMANCE.md`** documents the full progression, what the
    engine caches at runtime, and methodology.

### Changed

- **MSRV bumped** from `1.56.0` to `1.68` (real floor from
  `thiserror 2.0`, `regex 1.12`, `serde_json 1.0.149`).
- Template parser rewritten — close-delim search starts after the
  opening one (so `{{}}` no longer matches an empty key), nested
  `{{…{{…}}}}` is properly rejected, bare delimiter chars are
  treated as literal text.
- `engine::EngineError` and `error::EngineError` now resolve to the
  same definition; no more silent type mismatch between the two
  module paths.
- `.github/workflows/release.yml` delegated to
  `sebastienrousseau/pipelines/release.yml@99a39f7`, fires on
  `v*.*.*` tags only, includes a `verify` matrix on macOS / Linux /
  Windows.
- `Makefile` `test` target now runs default features,
  `remote-templates` features, and `--doc --all-features` in
  sequence — matches the `pre-push` hook.
- `deny.toml` allowlist kept broad (BSD, ISC, CC0-1.0, Unicode-3.0)
  to cover feature-gated deps; documented in-line.
- Dependabot: daily → weekly, grouped minor/patch, `chore(deps)`
  prefix.
- CI: seven per-job workflows (`audit`, `check`, `coverage`,
  `document`, `lint`, `release`, `test`) consolidated into one
  `ci.yml` that delegates to reusable workflows in
  `sebastienrousseau/pipelines`, pinned by SHA.

### Fixed

- `clippy::identity_op` on the 1 MiB download cap under
  `--features remote-templates`.
- `Makefile`: removed the broken `rustup component add rustfix`
  step — it never existed as a rustup component. `cargo fix` ships
  with the toolchain.
- `tests/error_tests.rs`: replaced `http://localhost:1` with
  `http://nonexistent.invalid./` (RFC 2606 reserved TLD) to prevent
  accidental mask-hits on developer machines.

### Removed

- **Default URL fallback** in `create_template_folder(None)` —
  previously downloaded six files from a hardcoded
  `raw.githubusercontent.com` URL.
- `build.rs` and `version_check` build-dep (Cargo enforces
  `rust-version` natively).
- Unused direct dependencies: `regex`, `serde`, `serde_json`.
- Placeholder `async` feature flag.
- `examples/example.rs` (shared-module wrapper with no unique
  behaviour).
- `.github/workflows/document.yml` — docs are served by docs.rs;
  the `gh-pages` branch was retired.
- Orphaned `.deepsource.toml` (no DeepSource integration was wired
  up).
- Duplicate `.github/CODE-OF-CONDUCT.md` + `.github/SECURITY.md`
  (root versions are canonical).

### Breaking changes

- **`Context::iter()`** now yields `(&String, &Value)` instead of
  `(&String, &String)`.
- **`Deref / DerefMut<Target = FnvHashMap<String, String>>`**
  removed from `Context`. Use `set_value` / `get_value` /
  `get_path` for typed and dot-notation access.
- **`Context::get`, `get_mut`, `remove`** return `Option<&String>`
  / `Option<&mut String>` / `Option<String>` only when the entry is
  a `Value::String`.
- **`TemplateError::EngineError(Box<EngineError>)`** removed
  (one-way `EngineError::Template(TemplateError)` retained).
- **`PageOptions`** removed — dead code, never wired into
  `render_page` / `render_template`. Removed along with the
  `engine::PageOptions` module path and the top-level
  `staticweaver::PageOptions` re-export.
- **`Cache::get`** is now `&mut self` to bump LRU access recency;
  `Cache<K, V>` adds `K: Clone` to its impl bounds.

### Security

- `#![forbid(unsafe_code)]` at the crate root.
- All HTTP paths moved behind the `remote-templates` cargo feature.
- `reqwest` dep tightened to `default-features = false` with
  `rustls-tls-native-roots` (drops OpenSSL pull-in).
- `cargo deny` license allowlist expanded to cover the full
  transitive dep graph; `[advisories] yanked = "deny"` added.
- `cargo update -p fastrand` from yanked 2.4.0 to 2.4.1.

## [0.0.1] - 2025-01-15

Initial public release.

[0.0.2]: https://github.com/sebastienrousseau/staticweaver/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/sebastienrousseau/staticweaver/releases/tag/v0.0.1
