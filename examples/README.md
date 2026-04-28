# StaticWeaver Examples

This directory contains examples of how to use the StaticWeaver library. Each
example is a standalone executable that demonstrates a specific feature or
group of features.

## Running the Examples

You can run any example using `cargo run --example <name>`. For example:

```bash
cargo run --example hello
```

## Available Examples

### [hello](hello.rs)
The simplest possible usage of StaticWeaver. Demonstrates basic template
rendering with string substitution.

```bash
cargo run --example hello
```

### [context](context.rs)
Detailed demonstration of the `Context` and `Value` types. Shows how to:
- Insert and retrieve basic strings.
- Store typed values (booleans, numbers, lists, maps).
- Use dot-notation for nested data lookups.
- Manage context capacity and clear entries.

```bash
cargo run --example context
```

### [engine](engine.rs)
Comprehensive guide to the `Engine` rendering capabilities. Covers:
- Basic string templates.
- HTML escaping and raw opt-out (`{{!key}}`).
- Template partials (`{{> partial}}`).
- Built-in filters (`uppercase`, `lowercase`, `trim`, `truncate`).
- Control flow blocks (`{{#if}}`, `{{#each}}`).
- File-backed rendering (`render_page`) with subdirectory support.
- Custom delimiters and cache management.

```bash
cargo run --example engine
```

### [cache](cache.rs)
Deep dive into the internal `Cache` mechanics used for `render_page`. Shows:
- Time-based expiration (TTL).
- Least Recently Used (LRU) eviction policy.
- Manual entry refreshing and updates.
- Safe iteration and consumption.

```bash
cargo run --example cache
```

### [errors](errors.rs)
Demonstrates the custom error types and safe result handling. Shows how
to catch and inspect common issues like unresolved tags or invalid paths.

```bash
cargo run --example errors
```

### [remote](remote.rs) (Requires `remote-templates` feature)
Shows how to fetch template files from a remote HTTP/S server into a local
directory before rendering.

```bash
cargo run --example remote --features remote-templates
```

### [axum](axum.rs) (Requires `axum-example` feature)
End-to-end Axum integration. Boots a minimal HTTP server demonstrating
three patterns:
- Render to `String` and return as `Html<String>`.
- Render to `Vec<u8>` via `Engine::render_to` for direct response-body
  streaming (the same shape works for any `std::io::Write` sink —
  Actix, Hyper channels, file writers).
- Per-request context from path parameters, with a custom filter
  registered via `Engine::add_filter`.

```bash
cargo run --example axum --features axum-example
# then open http://127.0.0.1:3030/
```

### [inheritance](inheritance.rs)
Template inheritance: `{{#extends}}` + `{{#block}}` with `{{ super() }}`.
Layered design where a base layout defines named blocks and a child
overrides them, splicing the parent body in via `super()`.

```bash
cargo run --example inheritance
```

### [filters](filters.rs)
Built-in filter pipeline (trim, uppercase, truncate, number_format,
pad_start, repeat, slice) plus custom filters and tests registered via
`Engine::add_filter` and `Engine::add_test`.

```bash
cargo run --example filters
```

### [loaders](loaders.rs)
Pluggable template loaders: `FsLoader` (default, reads from disk),
`MemoryLoader` (in-memory map for tests / embedded assets), and a
custom `LiveLoader` that hot-mutates its template store at runtime.

```bash
cargo run --example loaders
```

### [control_flow](control_flow.rs)
Expression language showcase: comparisons, booleans, math, string
concat (`~`), postfix tests (`is X`), `#each` over List + range,
`@index`/`@first`/`@last` helpers, `#break`/`#continue` loop control,
and `#set` in-template assignment.

```bash
cargo run --example control_flow
```

---

## Example Support

All examples use a shared `support.rs` module to provide a consistent visual
style with animated spinners and status checkmarks.
