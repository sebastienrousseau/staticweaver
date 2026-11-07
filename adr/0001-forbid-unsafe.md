# ADR-001: Forbid `unsafe` code

- Status: Accepted
- Date: 2026-04-29 (commit `20cffe0`, release v0.0.2)
- Issue: #54

## Context and Problem Statement

The initial commit set `unsafe_code = "allow"` in `Cargo.toml`
`[lints.rust]` (`762ca60`, 2024-10-17). The v0.0.2 release changed
it to `unsafe_code = "forbid"` and added `#![forbid(unsafe_code)]` at
the crate root (`20cffe0`). The question this record answers: should
the crate permit `unsafe` blocks at all?

## Considered Options

1. Allow `unsafe` (the state before `20cffe0`).
2. Forbid `unsafe` in the crate with `#![forbid(unsafe_code)]` and
   the matching `[lints.rust]` entry.

No other option is recorded in the repository.

## Decision Outcome

Chosen option 2. Evidence that the code reflects it at v0.0.9
(`71d204d`):

- `src/lib.rs` line 1 and `src/bin/staticweaver.rs` line 30:
  `#![forbid(unsafe_code)]`.
- `Cargo.toml` `[lints.rust]`: `unsafe_code = "forbid"`.

Rationale as recorded in the repository:

- It is listed first under "Hardening guarantees" in `SECURITY.md`,
  and memory safety is in that file's "Scope" section.
- `README.md` "Overview" presents it as "Pure Rust: no C bindings, no
  FFI, no `unsafe` blocks", and the "Ecosystem Comparison" table
  uses "Zero `unsafe`" as a differentiator against Tera, Minijinja,
  Handlebars and Askama.
- The `Cargo.toml` package `description` ends "zero unsafe code".

Why `forbid` was chosen over `deny`: Rationale not recorded;
maintainer to confirm.

### Escape valve

`forbid` cannot be overridden by an item-level `#[allow]`, so there
is no in-crate escape valve. No policy for a future exception (for
example a separate crate holding audited `unsafe`) is recorded.
Rationale not recorded; maintainer to confirm.

`PERFORMANCE.md` ("† `escape_heavy`: about the path change") scopes
a custom AVX2/NEON escape for a later release "if benchmarks justify
the complexity". How that would coexist with this decision is not
recorded; maintainer to confirm.

## Consequences

- Undefined behaviour can still enter through dependencies and macro
  output. The Miri job in `.github/workflows/ci.yml` exists for that
  reason: "Even with `#![forbid(unsafe_code)]` at the crate root,
  Miri catches UB introduced by derive-macro output (thiserror) and
  transitive deps (fnv, tempfile)" (comment added in `165daad`). The
  job is still `continue-on-error: true`, although the same comment
  says it becomes a hard gate from v0.0.5.
- Performance work is limited to safe Rust. The v0.0.3 escape path
  relies on autovectorisation of a safe loop (`PERFORMANCE.md`, "†
  `escape_heavy`"); see ADR-006.

## More Information

- `SECURITY.md`, "Hardening guarantees"
- `README.md`, "Overview", "Ecosystem Comparison", FAQ "Safety
  guarantees"
- `CHANGELOG.md`, `[0.0.2]` "Security"
- Commits: `762ca60`, `20cffe0`, `165daad`
