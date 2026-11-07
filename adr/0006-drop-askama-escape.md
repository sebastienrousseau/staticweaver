# ADR-006: Drop `askama_escape`

- Status: Accepted
- Date: 2026-06-27 (commits `4f7469d` and `19f6be9`, release v0.0.3)
- Issue: #54 (bug: sebastienrousseau/static-site-generator#589)

## Context and Problem Statement

v0.0.2 delegated HTML escaping in `escape_html_into` to the
`askama_escape` crate, chosen for its SIMD inner loop ("D2" in
`PERFORMANCE.md`, "Phase D progression"; `Cargo.toml` comment
removed in `19f6be9`). That call escaped every `&`
unconditionally, so a value already containing `&amp;` became
`&amp;amp;`. On one downstream site build, "1,437 pages contained
`&amp;amp;`" out of 4,992 (`4f7469d`). Entity-aware escaping needs
lookahead, which the `askama_escape` path did not provide
(`4f7469d`). Should the crate keep `askama_escape`?

## Considered Options

1. Keep `askama_escape` and accept non-idempotent escaping.
2. Replace it with a hand-written, entity-aware scalar scan.
3. A custom SIMD (AVX2/NEON) escape with entity lookahead, "scoped
   for v0.0.4 if benchmarks justify the complexity"
   (`PERFORMANCE.md`, "† `escape_heavy`").

## Decision Outcome

Chosen option 2. `4f7469d` replaced the body of `escape_html_into`
with an entity-aware scan; `19f6be9` removed `askama_escape` from
`Cargo.toml` and `Cargo.lock`. The #33 rewrite in v0.0.3 then made
it a byte-indexed scan with `matches!` over the five OWASP
characters and bulk `push_str` flushes (`CHANGELOG.md`, `[0.0.3]`
"Changed").

Rationale: idempotency, `escape(escape(x)) == escape(x)`. The SIMD
path re-escaped already-formed entities, "breaking idempotency"
(`PERFORMANCE.md`, "† `escape_heavy`"). The scalar path is "~50 LOC,
no new dependency, idempotency guaranteed" (same section).

Measurements (`escape_heavy`, 10 KB body, 5% metacharacters;
`PERFORMANCE.md`, "† `escape_heavy`" table; `CHANGELOG.md`
`[0.0.3]`):

| Path | Time |
| :--- | ---: |
| v0.0.2, `askama_escape` SIMD | 23.3 µs |
| Scalar `char_indices` scan (after ssg#589, before #33) | 78.3 µs |
| v0.0.3 byte-indexed scan (#33) | 26.2 µs |

The 26.2 µs figure was measured on 2026-06-27 on an Apple M1 Pro,
macOS 26.5.1, rustc 1.95.0, release profile `lto = true`,
`opt-level = "z"` (`PERFORMANCE.md`, "Headline numbers" and
"Hardware and toolchain"). `4f7469d` states "benchmarks unchanged in
noise" for the intermediate scan, while `CHANGELOG.md` `[0.0.3]`
reports a 3.4× regression for it. The regression is the correct
account. Issue #33 was opened to benchmark `4f7469d` and `19f6be9`,
and its closing comment records "regressed to 78.3 µs (3.4× over the
23.3 µs SIMD baseline)"; 78.3 / 23.3 = 3.36.

A re-run on 2026-11-07 agrees on the direction. Each revision was
checked out in a temporary `git worktree`, built with its own
`Cargo.lock` (`cargo bench --locked --bench comparative --no-run`),
and its bench binary run as `<binary> --bench --noplot
--warm-up-time 2 --measurement-time 5 escape_heavy`, three rounds
interleaved across revisions. Machine: Apple A18 Pro (`Mac17,5`, 6
cores), macOS 27.2, rustc 1.99.0 for every revision, each
revision's own release profile (`lto = true`, `opt-level = "z"`).
The machine was heavily loaded (1-minute load average 337 to 441
during the runs), so absolute times are about 15× the June figures
and only the ratios carry information. Median Criterion point
estimate of `escape_heavy/staticweaver` over the three rounds:

| Revision | Median | vs v0.0.2 | vs Askama, same run (per round) |
| :--- | ---: | ---: | :--- |
| v0.0.2 (`askama_escape`) | 360 µs | 1.00× | 0.99, 0.99, 0.99 |
| `4f7469d` (scalar `char_indices`) | 1,475 µs | 4.09× | 4.18, 3.63, 5.53 |
| `19f6be9` (dependency dropped) | 1,544 µs | 4.28× | 3.31, 4.45, 4.86 |
| v0.0.3 (byte-indexed, #33) | 540 µs | 1.50× | 1.23, 1.48, 1.33 |
| `209dd2e` (v0.0.10, `main`) | 592 µs | 1.64× | 1.81, 1.54, 1.30 |

Every round shows `4f7469d` at least 3.3× slower than v0.0.2, so
"unchanged in noise" does not hold. Under this load the run does not
pin the exact factor (3.4× in #33 against about 4× here), nor the
v0.0.3 recovery (12.5% in #33 against 23–48% here); the Askama
column also compares against different Askama releases (0.12.1,
0.15.6, 0.15.6, 0.16.0, 0.16.1 by row).

Option 3 was never attempted. No commit on any branch adds
`std::arch`, `core::arch`, `target_feature` or
`is_x86_feature_detected` (`git log --all -S`, checked 2026-11-07 at
`209dd2e`), the CHANGELOG mentions SIMD only for v0.0.2 and its
removal, and no issue or pull request proposes it beyond #33 and
#34. ADR-001 constrains it.

## Consequences

- No runtime escape dependency. At v0.0.9, `askama_escape` is absent
  from `Cargo.toml` and `Cargo.lock`. `askama` 0.16 remains, but
  only under `[dev-dependencies]`, for `cargo bench --bench
  comparative` (`Cargo.toml` comment: "Comparative benchmarks").
- Idempotency is defended by property tests (#31) and Kani proofs in
  `src/engine.rs` (`SECURITY.md`, "Hardening guarantees").
- `escape_heavy` is about 12.5% slower than the SIMD baseline.
- `.github/dependabot.yml` still carries an `ignore` rule and comment
  for `askama_escape`, which is no longer a dependency.

## More Information

- `PERFORMANCE.md`, "† `escape_heavy`: about the path change",
  "Phase D progression"
- `CHANGELOG.md`, `[0.0.3]`
- Commits: `20cffe0`, `00b59ae`, `4f7469d`, `19f6be9`, `8bf88c2`
