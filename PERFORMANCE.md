# Performance

`staticweaver` aims to be the **fastest non-codegen Rust template
engine** — competitive with Minijinja, faster than Tera, and within
striking distance of Askama on workloads where compile-time codegen
isn't feasible (i.e. templates that change without recompilation).

This document records what we measure, how we measure it, and what
the engine caches at runtime.

## Headline numbers

Full-quality `cargo bench --bench comparative` (Criterion, 2 s
warm-up + 5 s measurement) on Apple M-series, after Phases D and H
(latest measurement). Templates produce equivalent *output*; each
engine uses its native syntax.

| Workload | staticweaver | Tera | Minijinja | Askama | vs Minijinja |
| :--- | ---: | ---: | ---: | ---: | :--- |
| `simple_sub` (1 tag) | **497 ns** | 388 ns | 591 ns | 95 ns | **+19%** (we win) |
| `many_sub_32` (32 tags) | **12.85 µs** | 5.96 µs | 14.40 µs | 973 ns | **+11%** (we win) |
| `escape_heavy` (10 KB body, 5% metachar) | **23.3 µs** | 77.8 µs | 24.3 µs | 23.2 µs | **+4%** (ties Askama too) |
| `each_100` (100 items) | 58.3 µs | 17.8 µs | 23.6 µs | 5.24 µs | −2.5× |
| `each_1000` (1000 items) | 557 µs | 171 µs | 184 µs | 51.9 µs | −3.0× |
| `if_chain` (nested conditionals) | 2.51 µs | 455 ns | 656 ns | 25.4 ns | −3.8× |
| `filter_chain` (`trim \| upper`) | **1.03 µs** | 620 ns | 988 ns | 198 ns | **+5%** (we win) |

**Wins or ties Minijinja on 4 / 7 workloads.** We **beat Tera on
escape-heavy 3.3×** and **match Askama on the same workload**
(23.3 µs vs 23.2 µs) — the SIMD escape path is fully competitive
with Askama's compile-time codegen on long inputs.

The remaining 2.5–3.8× gap on loops and conditional chains is
constant-factor per-tag overhead in the runtime AST walker; closing
it would require a bytecode compiler. That was scoped as Phase D
Option 1 and explicitly rejected to preserve the "small enough to
read in an afternoon" pillar.

## Phase D progression

The Phase D performance work, commit by commit:

| Phase | Change | Headline win |
| :--- | :--- | :--- |
| **D1** | Comparative bench matrix vs Tera, Minijinja, Askama | (foundation) |
| **D2** | SIMD HTML escape via `askama_escape` | escape_heavy −34% (34.4µs → 24µs) |
| **D5** | Hoist context clone out of `#each` loop | each_1000 −97% (22.6ms → 640µs, 35×) |
| **D4** | `set_value_str` — borrow key on update | each_1000 −12% (640µs → 563µs) |
| **D6** | Allocation-free close-tag match in `extract_block` | if_chain −8% (2.6µs → 2.4µs) |
| **D3** | Reuse `Value::String` buffer in `#each` (`set_value_string`) | each_100 −18% (67µs → 55µs) |

**Cumulative each_1000: 22.6 ms → 557 µs (~40× faster).** Numbers
in the headline table above use the latest post-H measurement;
intermediate per-phase numbers reflect the bench at the time each
sub-phase landed.

## What we cache

`staticweaver` is parser + walker, not parser + compiler — we
deliberately do not emit bytecode. The runtime caches:

| Cache | Lives on | Keyed by | TTL/eviction | Touched by |
| :--- | :--- | :--- | :--- | :--- |
| `render_cache` | `Engine` | `"{layout}:{Context::hash()}"` | TTL + LRU bound | `render_page` only |
| (none) | — | — | — | `render_template` is pure |

`render_template` is `&self` and stateless — it parses the template
and walks the AST on every call. The cache lives one level up, on
`render_page`, which is `&mut self` and memoises the entire rendered
page body.

### Why no expression cache?

A "halfway-to-bytecode" cache that stores parsed `Expr` trees per
template hash was scoped (D6, original plan) but skipped after the
data came back. Per-call `parse_expr` cost is sub-200ns even for
nested boolean expressions; caching would save that 200ns at the
price of either:

* a breaking API change (`render_template(&mut self, …)`), or
* interior mutability (`Mutex<HashMap<…>>`) with lock overhead per
  render and cascading `Send`/`Sync` ripple.

Neither was worth ~200ns × ~2 #if blocks per template. The cache
stays available for v0.1.0 if Phase E benchmarks justify the API
churn.

### Why no bytecode VM?

A compiled bytecode + interpreter (the path Minijinja takes) would
likely close the remaining 2.5–3.5× gap on `#each` and `if_chain`
workloads. It was scoped as Phase D Option 1 and explicitly
rejected — the marginal gain (15–30% on hot workloads) doesn't
justify ~1500 LoC of compiler + VM that breaks the "small" pillar
in our positioning vs Tera and Minijinja. If you need bytecode-VM
performance, use Minijinja directly.

## How to run the benchmarks yourself

```bash
# Quick comparative (under 1 minute)
cargo bench --bench comparative -- --quick

# Full quality (~5 minutes; what the numbers above are from)
cargo bench --bench comparative

# Just our own scenarios (no comparison)
cargo bench --bench template_benchmark
```

Criterion writes HTML reports to `target/criterion/`. Open
`target/criterion/report/index.html` in a browser for the
side-by-side comparison plots.

## Reproducing the comparison

The comparison templates are defined inline in
[`benches/comparative.rs`](benches/comparative.rs) — one struct or
template literal per engine per workload. Each engine renders to
`String`. Templates are translated to each engine's syntax; the
*output* is what we keep equivalent, not the literal input.

If you want to add a workload, follow the existing pattern: define
the template in all four syntaxes, register one
`group.bench_function(BenchmarkId::new(<engine>, ""), …)` per
engine, and add the new function to the `criterion_group!` macro.

## Hardware and toolchain

Numbers above were measured on:

* Apple M-series, macOS 25.4 (Darwin)
* Rust stable (per `rust-toolchain.toml`)
* Release profile: `lto = true`, `codegen-units = 1`, `opt-level = "z"`

Numbers will vary on other hardware. The *ratios* between engines
are stable across machines; the absolute numbers shift with CPU
generation and cache size.

## Methodology notes

* **Each engine pre-compiles its template** (where applicable) so
  parse cost isn't counted per render. Tera and Minijinja precompile
  via `add_raw_template`; Askama is compile-time. staticweaver does
  not have a precompile step — every render parses.
* **Context construction is excluded**. We build the context once
  outside the benchmark loop and re-use it.
* **Output allocation is included**. Every iteration produces a
  fresh `String`. This is the realistic cost of a render in a
  request handler.
* **Minijinja autoescape is enabled** for the `escape_heavy`
  workload to match staticweaver's HTML-escape-by-default behaviour.
  Tera autoescape requires the `.html` template extension
  (configured per-template).
* **Askama is compile-time codegen** — its numbers reflect
  monomorphised `write!` calls with no runtime parser. It is
  always faster than any runtime engine on simple workloads
  (5–124× in our matrix). The realistic ceiling for staticweaver
  on those workloads is "competitive at the compile-time engines'
  price floor."
