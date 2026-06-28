# Changelog

All notable changes to `staticweaver` are documented in this file. See
[README.md](README.md) for an overview of the engine's capabilities, the
[Templating Language](README.md#templating-language) reference, and the
public API.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-`1.0.0`, breaking changes may occur in minor/patch releases and are
called out explicitly below).

## [Unreleased]

## [0.0.4] - 2026-06-28

### Breaking
- `Engine::render_page`, `render_page_to`, `set_max_cache_size`,
  `clear_cache` now take `&self` (was `&mut self`). Issue #36. Enables
  sharing one `Arc<Engine>` across threads / async tasks without the
  `Arc<Mutex<Engine>>` envelope. Direct callers of `engine.render_cache`
  must now go through `engine.render_cache.lock().unwrap()` because the
  field is wrapped in `std::sync::Mutex` to provide the interior
  mutability the cache needs.
- **MSRV** raised 1.68 → 1.75 to enable async fn in traits (AFIT) used
  by the optional `async` feature. The sync surface would still build on
  1.68, but the documented floor is 1.75 to keep the contract honest.

### Added
- **Concurrency**: `Engine: Send + Sync + Clone`. New `tests/concurrent_render.rs`
  with 6 compile-time + runtime soak tests proving 8 threads × 10 000
  renders share one `Arc<Engine>` without data races (#36).
- **Async**: new `async` and `async-tokio` features. `AsyncTemplateLoader`
  trait, `TokioFsLoader` + `MemoryAsyncLoader` impls, `Engine::render_template_async`
  / `render_page_async` / `render_to_async` / `render_page_to_async`
  methods. Reuses the same `Mutex<Cache<…>>` as the sync path (#37, #38).
- **Observability**: new optional `tracing` feature. `Engine::render_template`
  and `render_page` emit `tracing::instrument` spans
  (`staticweaver.render_template`, `staticweaver.render_page`) with
  template-length / layout / context.len fields (#39).
- **Cache metrics**: new `CacheStats` struct (Copy + Default + Eq) +
  `Cache::stats()` method exposing `inserts`, `hits`, `misses`,
  `evictions`, `ttl_expired` counters. Designed for cheap export to
  prometheus/metrics/opentelemetry. 7 new unit tests (#40).
- **Fuzzing**: new standalone `fuzz/` crate with three libfuzzer
  targets (`parse`, `escape`, `dot_path`) + `.github/workflows/fuzz.yml`
  nightly job (#41).
- **Miri CI job**: nightly `cargo miri test --lib` (`continue-on-error`
  for v0.0.4, hard gate planned for v0.0.5) (#42).
- **Kani formal verification**: `#[cfg(kani)]` module in `src/engine.rs`
  with two proofs against `escape_html_into` (idempotency + no bare
  metachars). Weekly `.github/workflows/kani.yml` (#43).
- **Supply chain**:
  - SBOM + Sigstore on release — `release.yml` now generates CycloneDX
    + SPDX JSON, signs the `.crate` artifact via cosign keyless OIDC,
    attaches everything to the GitHub Release (#44).
  - `cargo-vet init`: `supply-chain/config.toml` with imports for
    Mozilla, Google, bytecode-alliance, Embark. CI gate (#45).
  - OSV-Scanner CI job (GHSA + RUSTSEC + crates.io coverage) (#46).
- **Version-drift CI gate**: `scripts/check-version-consistency.sh`
  enforces that every `staticweaver = "x.y.z"` snippet in `README.md`
  matches `Cargo.toml`. Added after the v0.0.3 release shipped with
  README install snippets stuck at `0.0.2`.

### Changed
- `[lints.rust] missing_docs = "deny"` (was `warn`) — 100% rustdoc
  coverage is now a `cargo build` failure, not just a CI check.

## [0.0.3] - 2026-06-27

### Fixed
- HTML-entity escape is now idempotent — `escape(escape(x)) == escape(x)`.
  Closes sebastienrousseau/static-site-generator#589. Defended by three
  new property tests (#31): idempotency, no bare angle brackets in output,
  every `&` begins a valid entity reference.
- `Context::hash()` is no longer collision-prone (#30). Keys are sorted
  before being fed to the hasher; the previous XOR aggregation could
  produce identical digests for distinct `(key, value)` sets, returning
  a stale render from the `render_page` cache. The same fix is applied
  inside `hash_value` for nested `Value::Map` entries.

### Added
- Opt-in lax mode for unresolved template tags (#28). Strict mode remains
  the default; lax mode emits `""` for any unresolved `{{key}}` and skips
  the attached filter chain. New `tests/lax_mode.rs` matrix locks the
  wire format with 10 strict/lax/differential cases (#32).

### Changed
- HTML escape path rewritten as an inline byte-indexed scan with
  `matches!` over the OWASP 5-char set and bulk-flush via `push_str`
  (#33). Recovers from the 3.4× regression introduced by the
  `askama_escape` removal: `escape_heavy` 78.3 µs → **26.2 µs**
  (−66.6 %), within ~12.5 % of the pre-ssg#589 SIMD baseline (23.3 µs)
  while preserving the idempotency invariant.
- `PERFORMANCE.md` re-stamped (#34) with date / toolchain / CPU on every
  measurement; new Phase-#33 row added to the progression table; the
  `escape_heavy` claim re-labelled to reflect the scalar entity-aware
  path.

### Removed
- Direct dependency on `askama_escape`. The new inline escape path is
  hand-rolled — no new runtime dependency added. See PERFORMANCE.md
  for measured impact.

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
- `criterion` dev-dep bumped `0.5` → `0.8`. Bench files migrated
  to `std::hint::black_box` (the v0.8 deprecation of the
  `criterion::black_box` re-export). `cargo bench --bench
  comparative` runs cleanly under v0.8 with no perf regression.
  Closes Dependabot #13.
- `reqwest` (feature-gated) bumped `0.12` → `0.13`. The TLS
  feature flag was renamed in v0.13: `rustls-tls-native-roots` →
  `rustls` + `rustls-native-certs`. Same behaviour — TLS via
  rustls (no OpenSSL pull-in), system root certs honoured.
  Default builds remain HTTP-free. Closes Dependabot #12.
- `actions/checkout` workflow action bumped `v4` → `v6` across
  `ci.yml` (3 references) and `release.yml` (1 reference).
  Closes Dependabot #9.
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
