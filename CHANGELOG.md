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
  line coverage drops below 95%. `make coverage` produces the same
  report locally.
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
