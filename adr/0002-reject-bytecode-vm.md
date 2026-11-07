# ADR-002: Reject a bytecode VM

- Status: Accepted
- Date: 2026-04-29 (commit `20cffe0`, which added `PERFORMANCE.md`
  in release v0.0.2)
- Issue: #54

## Context and Problem Statement

staticweaver is "parser + walker, not parser + compiler": every
`render_template` call parses the template and walks the AST
(`PERFORMANCE.md`, "What we cache"). The comparative benchmarks
show it behind Minijinja and Tera on loop and conditional workloads
(`PERFORMANCE.md`, "Headline numbers": `each_100`, `each_1000`,
`if_chain`). `PERFORMANCE.md` attributes the gap to "constant-factor
per-tag overhead in the runtime AST walker; closing it would require
a bytecode compiler". Should the engine compile templates to bytecode
and run them on an interpreter?

## Considered Options

1. Bytecode compiler + VM ("Phase D Option 1", the path Minijinja
   takes).
2. Keep the AST walker and cut constant factors instead (Phase D
   items D2 to D6 in `PERFORMANCE.md`, "Phase D progression").
3. A per-template expression cache ("halfway-to-bytecode"), covered
   below.

## Decision Outcome

Chosen option 2. Option 1 "was scoped as Phase D Option 1 and
explicitly rejected" (`PERFORMANCE.md`, "Why no bytecode VM?").

Trade-off as recorded in `PERFORMANCE.md`, "Why no bytecode VM?":

- Expected gain: "15–30% on hot workloads".
- Cost: "~1500 LoC of compiler + VM that breaks the 'small' pillar
  in our positioning vs Tera and Minijinja".
- Guidance to users: "If you need bytecode-VM performance, use
  Minijinja directly." `README.md` FAQ repeats this in its "which
  engine" table.

How the 15–30% gain and the ~1500 LoC figure were estimated:
Measurement not recorded; maintainer to confirm. The same section
also says a VM "would likely close the remaining 2.5–3.5× gap",
which does not obviously agree with a 15–30% gain; maintainer to
confirm which figure is current.

Option 3 was also skipped: per-call `parse_expr` cost is "sub-200ns",
and caching it needed either a breaking `&mut self` API or a
`Mutex<HashMap<…>>` (`PERFORMANCE.md`, "Why no expression cache?").

## Consequences

- staticweaver remains slower than Minijinja on loops and nested
  conditionals ("−2.5×" to "−3.8×" in the headline table).
- The "small" claim must be kept true for this trade-off to hold.
  `README.md` says "~5k LoC engine" in its FAQ but "~10.7k lines
  across 6 modules" under "Project metrics"; maintainer to confirm
  which figure the pillar is measured against.
- Constant-factor work continues instead (D2 to D6, #33).

## More Information

- `PERFORMANCE.md`, "Headline numbers", "Phase D progression", "What
  we cache", "Why no expression cache?", "Why no bytecode VM?"
- `README.md`, FAQ comparison table
- Commit: `20cffe0`
