# ADR-004: Workspace split (core / std / async / cli)

- Status: Accepted
- Date: 2026-11-07 (release v0.0.10)
- Issue: #54 (the split itself: #48; follow-ups #49, #50)

## Context and Problem Statement

Up to v0.0.9 staticweaver was one crate: the parser, renderer,
filters, HTML escape, `Value`, errors, `Engine`, filesystem loader,
render cache, async loader and the CLI binary all lived under `src/`
(`src/engine.rs` alone was 8,823 lines at `71d204d`). Issue #48 states
the problem: the single-crate layout "blocks `no_std` and traps async
behind cfg-gates". The render path uses `std` only at its edges
(`Duration` for the cache TTL, `std::fs` in `FsLoader`, `std::io` in
`render_to` and `EngineError::Io`), so the templating core could serve
`no_std + alloc` targets if those edges lived elsewhere (issue #49).

How should the code be divided so that the core can build without
`std`, while users of `staticweaver` 0.0.9 see no source change?

## Considered Options

1. Keep one crate and gate `std` use behind a `std` feature.
2. Split into a workspace: `staticweaver-core` (no_std + alloc),
   `staticweaver` (std facade), `staticweaver-async`,
   `staticweaver-cli` (the layout issue #48 prescribes).

Option 1 is not discussed as an alternative, but issue #48 states
the objection to the single-crate layout it describes: it "blocks
`no_std` and traps async behind cfg-gates", and the split is asked
for "so each layer evolves independently" (#48, "Summary"). Keeping
one crate behind a `std` feature would keep async and std behind
those cfg-gates, which is the reason #48 gives for splitting.

## Decision Outcome

Chosen option 2, implemented on `feat/v0.0.10`:

- **`crates/staticweaver-core`**: `context` (`Value`, `Context`, and a
  new `Map` alias), `error` (`EngineError`, `TemplateError`), `path`
  (`validate_name`, `resolve_template_path`), the expression parser
  and evaluator, the filter pipeline, the escape (with the Kani
  proofs), and a new `Renderer` that borrows its configuration. A
  default `std` feature keeps the 0.0.9 types (`Value::Map` is
  `FnvHashMap`, `EngineError::Io` exists); without it `Map` is a
  `BTreeMap` and the crate is `#![no_std]` with `alloc`.
- **`crates/staticweaver`**: `Engine` with its public fields
  unchanged, `FsLoader`, `MemoryLoader`, the TTL/LRU cache, the
  remote-template fetcher. `Engine` builds a core `Renderer` per call.
  It re-exports the core and async items at their old paths
  (`staticweaver::context`, `staticweaver::error`,
  `staticweaver::engine::{TemplateLoader, FilterFn, TestFn}`,
  `staticweaver::loader_async`), so `cargo add staticweaver` gives
  the same API.
- **`crates/staticweaver-async`**: `AsyncTemplateLoader`,
  `MemoryAsyncLoader`, and `TokioFsLoader` behind its `tokio` feature.
  The facade's `async` / `async-tokio` features enable it, which keeps
  tokio out of the default tree (ADR-003).
- **`crates/staticweaver-cli`**: the `staticweaver` binary, moved from
  `src/bin/staticweaver.rs`.
- Internal dependencies are pinned to the exact workspace version
  (`=0.0.10`), so the four crates release together. Issue #48 asked
  for exact pins.
- Tests, benches and examples moved into the crate they exercise.
  The CLI smoke tests moved to `staticweaver-cli`.

### Consequences

- Good: `cargo build -p staticweaver-core --no-default-features
  --target thumbv7em-none-eabihf` (and `wasm32-wasip2`) builds.
- Good: the 600-line render function became one function per tag
  kind while moving; behaviour is pinned by the existing test suites.
- Bad: `Value::Map`'s concrete type depends on the core's `std`
  feature, the same trade-off `serde_json::Map` makes with
  `preserve_order`. Code that must build both ways uses
  `Map::default()`.
- Bad: `cargo install staticweaver` no longer installs a binary; the
  CLI is `cargo install staticweaver-cli`.
- Bad: four crates to publish per release, in dependency order (core,
  async, staticweaver, cli), and each needs its own crates.io
  trusted-publisher entry.

## More Information

- Issue #48 (target layout and acceptance criteria), issue #49
  (no_std criteria).
- `CHANGELOG.md`, section `[0.0.10]`.
- The workspace manifest: `Cargo.toml` at the repository root.
