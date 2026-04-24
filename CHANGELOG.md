# Changelog

All notable changes to `staticweaver` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-`1.0.0`, breaking changes may occur in minor/patch releases and are
called out explicitly below).

## [Unreleased]

### Added

- `Content-Type` validation on remote template fetches — reject responses
  whose MIME type does not look textual (non-`text/*`, non-JavaScript,
  non-JSON, non-XHTML).
- `#[cfg_attr(docsrs, doc(cfg(feature = "remote-templates")))]` on every
  feature-gated item so docs.rs renders the "available on crate feature
  remote-templates only" badge.
- `CHANGELOG.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`.
- **Cross-platform CI** — `run-cross-platform: true` in `ci.yml` fans every
  PR to macOS + Windows runners. Multi-OS `verify` job in `release.yml`.
- **Portable git hooks** — repo-local `.githooks/{pre-commit,commit-msg,pre-push}`
  (POSIX `sh`). Installed by `make init`; enforces `commit.gpgsign=true`,
  Conventional-Commits subjects, and runs the full test battery before
  `git push`.
- **`rust-toolchain.toml`** pinned to `stable` with `rustfmt`+`clippy`.
- **Mock-server integration tests** — 6 new tests in `tests/download_tests.rs`
  covering the remote-templates HTTP path (happy path, 404, bad
  `Content-Type`, oversized `Content-Length`, JavaScript MIME acceptance,
  missing Content-Type tolerance). Uses `mockito` as a dev-dep.
- **New unit tests** for `Cache::IntoIterator` (live + expired), every
  `create_template_folder` branch (None, missing path, existing path, URL
  without feature), the `render_page` cache-hit path, the `"` escape
  branch, and `set_max_cache_size`'s no-op path.
- **Coverage gate** — `coverage-gate` CI job fails the build if line
  coverage drops below 95%. `make coverage` produces the same report
  locally.
- **`examples/remote.rs`** — feature-gated example demonstrating
  `create_template_folder(Some(url))` against a local `mockito` server.

### Fixed

- `clippy::identity_op` on the 1 MiB download cap under
  `--features remote-templates`.
- `Makefile`: remove the broken `rustup component add rustfix` step — it
  never existed as a rustup component. `cargo fix` ships with the
  toolchain.
- `tests/error_tests.rs`: replace `http://localhost:1` with
  `http://nonexistent.invalid./` (RFC 2606 reserved TLD) to prevent
  accidental mask-hits on developer machines.

### Changed

- `.github/workflows/release.yml`: delegated to
  `sebastienrousseau/pipelines/release.yml@99a39f7`, fires on `v*.*.*`
  tags only, includes a `verify` matrix on macOS / Linux / Windows.
- `Makefile` `test` target now runs default features, `remote-templates`
  features, and `--doc --all-features` in sequence — matches the
  `pre-push` hook.
- `deny.toml`: allowlist kept broad (BSD, ISC, CC0-1.0, Unicode-3.0) to
  cover feature-gated deps; documented in-line.

### Removed

- Orphaned `.deepsource.toml` (no DeepSource integration was wired up).
- Duplicate `.github/CODE-OF-CONDUCT.md` + `.github/SECURITY.md` (root
  versions are canonical).

## [0.0.2] - 2026-04-24

### Added

- HTML-escape by default in `Engine::render_template` / `render_page` —
  values are escaped for `&<>"'`. Per-tag opt-out with `{{!key}}`; global
  opt-out with `Engine::new(...).with_html_escape(false)`.
- Layout-name validation in `render_page` — rejects `/`, `\`, `..`, and
  null bytes before touching the filesystem.
- `remote-templates` cargo feature — fetching templates via HTTP/S is
  now opt-in; default build has no networking code.
- Bounded HTTP downloads — 1 MiB per-file cap enforced against both
  `Content-Length` and the actual read.
- `with_html_escape(bool)` builder method on `Engine`.
- Whitespace trimming around tag keys — `{{ name }}` and `{{name}}` are
  equivalent.
- 100% doc coverage with examples across every public item; doctests
  exercised in CI under `-D rustdoc::broken_intra_doc_links`.
- Shared `examples/support.rs` spinner/checkmark helpers; all examples
  renamed to one-word filenames (`hello`, `context`, `cache`, `engine`,
  `errors`).
- `.github/labeler.yml` for automatic PR triage by path glob.
- Docs CI job: `cargo doc` under strict flags + doctest run + 100%
  example coverage gate.

### Changed

- **MSRV bumped** from `1.56.0` to `1.68` (real floor from `thiserror 2.0`,
  `regex 1.12`, `serde_json 1.0.149`).
- `Context::hash` now sorts keys before hashing so equal logical contexts
  always produce equal hashes (fixes `render_page` cache thrashing).
- Template parser rewritten — close-delim search starts after the opening
  one (so `{{}}` no longer matches an empty key), nested `{{…{{…}}}}` is
  properly rejected, bare delimiter chars are treated as literal text.
- `engine::EngineError` and `error::EngineError` now resolve to the same
  definition; no more silent type mismatch between the two module paths.
- Dependabot: daily → weekly, grouped minor/patch, `chore(deps)` prefix.
- CI: seven per-job workflows (`audit`, `check`, `coverage`, `document`,
  `lint`, `release`, `test`) consolidated into one `ci.yml` that delegates
  to reusable workflows in `sebastienrousseau/pipelines`, pinned by SHA.

### Removed

- Default URL fallback in `create_template_folder(None)` — previously
  downloaded six files from a hardcoded `raw.githubusercontent.com` URL.
- `build.rs` and `version_check` build-dep (Cargo enforces `rust-version`
  natively).
- Unused direct dependencies: `regex`, `serde`, `serde_json`.
- Placeholder `async` feature flag.
- `examples/example.rs` (shared-module wrapper with no unique behaviour).
- `.github/workflows/document.yml` — docs are now served by docs.rs; the
  `gh-pages` branch was deleted on the remote.

### Security

- Added `#![forbid(unsafe_code)]` at the crate root.
- All HTTP paths moved behind the `remote-templates` cargo feature.
- `reqwest` dep tightened to `default-features = false` with
  `rustls-tls-native-roots` (drops OpenSSL pull-in).
- `cargo deny` license allowlist expanded to cover the full transitive
  dep graph; `[advisories] yanked = "deny"` added.
- `cargo update -p fastrand` from yanked 2.4.0 to 2.4.1.

## [0.0.1] - 2025-01-15

Initial public release.

[Unreleased]: https://github.com/sebastienrousseau/staticweaver/compare/v0.0.2...HEAD
[0.0.2]: https://github.com/sebastienrousseau/staticweaver/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/sebastienrousseau/staticweaver/releases/tag/v0.0.1
