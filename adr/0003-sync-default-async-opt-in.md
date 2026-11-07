# ADR-003: Sync by default, async opt-in

- Status: Accepted
- Date: 2026-06-28 (commit `2a3e7c2`, release v0.0.4)
- Issue: #54 (async surface: #37, #38)

## Context and Problem Statement

The render path is "parser + tree-walk, not network": synchronous and
CPU-bound, with no I/O on the hot path (`README.md` FAQ, "Is
staticweaver async?"). Async callers still need to load templates
without blocking their executor; without an async loader they would
have to use `tokio::task::spawn_blocking`, "which defeats the point
of being in an async runtime" (`src/loader_async.rs` module doc).
How should the crate serve async callers without making every user
pay for an async runtime?

## Considered Options

1. Async-first API, with `tokio` as a default dependency.
2. Sync API by default; an optional, runtime-agnostic `async`
   feature; a separate `async-tokio` feature for the tokio-backed
   implementation.
3. Sync only (the state before v0.0.4).

## Decision Outcome

Chosen option 2, added in `2a3e7c2`. Evidence at v0.0.9 (`71d204d`):

- `Cargo.toml` `[features]`: `default = []`; `async = []`;
  `async-tokio = ["async", "dep:tokio"]`.
- `Cargo.toml` `[dependencies]`: `tokio` is `optional = true`, so it
  is not in the default dependency tree. Its only other activator is
  `axum-example`, which builds `examples/axum.rs` (tokio has been
  optional since it was added for that example in `20cffe0`).

Rationale as recorded:

- "Gated behind the `async` feature so default builds stay
  dep-light" (`src/loader_async.rs`).
- `TokioFsLoader` sits behind `async-tokio` "so callers running on
  other executors (async-std, smol, glommio) aren't forced to pull
  in tokio" (`src/loader_async.rs`).
- "Forcing an async runtime onto callers who don't need one is a net
  loss in dependency weight and complexity" (`README.md` FAQ, "Is
  staticweaver async?").
- The `Cargo.toml` comment on `async-tokio`: "zero weight on default
  builds".

## Consequences

- The async methods load asynchronously but render synchronously
  ("async load, sync render", commit `2a3e7c2`). They share the
  sync path's `Mutex<Cache>`, so a sync warm-up gives an async cache
  hit and the reverse (`CHANGELOG.md`, `[0.0.4]` "Added").
- `AsyncTemplateLoader` uses async fn in traits, which needed Rust
  1.75. That raised the MSRV from 1.68 to 1.75 in v0.0.4
  (`CHANGELOG.md`, `[0.0.4]` "Breaking"); the current 1.89 floor
  covers it (`Cargo.toml` `rust-version` comment).
- The tokio feature list in `Cargo.toml` is the union needed by both
  `axum-example` and `async-tokio`; with only `async-tokio` on, the
  `net` and `macros` parts are unused (comment above `tokio` in
  `Cargo.toml`).
- The blocking `reqwest` remote fetcher is unchanged; async callers
  wrap it in `spawn_blocking` (`README.md` FAQ).

## More Information

- `Cargo.toml`, `[features]` and `[dependencies]` comments
- `src/loader_async.rs`, module documentation
- `README.md`, "Optional features", FAQ "Is staticweaver async?"
- `CHANGELOG.md`, `[0.0.4]`
- Commits: `20cffe0`, `2a3e7c2`
