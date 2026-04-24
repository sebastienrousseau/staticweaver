<p align="center">
  <img src="https://kura.pro/staticweaver/images/logos/staticweaver.svg" alt="StaticWeaver logo" width="128" />
</p>

<h1 align="center">StaticWeaver</h1>

<p align="center">
  <strong>A powerful and flexible templating engine and static site generator for Rust.</strong>
</p>

<p align="center">
  <a href="https://github.com/sebastienrousseau/staticweaver/actions"><img src="https://img.shields.io/github/actions/workflow/status/sebastienrousseau/staticweaver/ci.yml?style=for-the-badge&logo=github" alt="Build" /></a>
  <a href="https://crates.io/crates/staticweaver"><img src="https://img.shields.io/crates/v/staticweaver.svg?style=for-the-badge&color=fc8d62&logo=rust" alt="Crates.io" /></a>
  <a href="https://docs.rs/staticweaver"><img src="https://img.shields.io/badge/docs.rs-staticweaver-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" alt="Docs.rs" /></a>
  <a href="https://codecov.io/gh/sebastienrousseau/staticweaver"><img src="https://img.shields.io/codecov/c/github/sebastienrousseau/staticweaver?style=for-the-badge&logo=codecov" alt="Coverage" /></a>
  <a href="https://lib.rs/crates/staticweaver"><img src="https://img.shields.io/badge/lib.rs-v0.0.2-orange.svg?style=for-the-badge" alt="lib.rs" /></a>
</p>

---

## Install

```bash
cargo add staticweaver
```

Or add to `Cargo.toml`:

```toml
[dependencies]
staticweaver = "0.0.2"
```

You need [Rust](https://rustup.rs/) 1.68 or later. Works on macOS, Linux, and Windows.

---

## Overview

StaticWeaver is a templating engine that resolves `{{ variables }}` against a context and outputs static files.

- **Variable substitution** with `{{ }}` syntax
- **Template caching** for fast repeated renders
- **Nested contexts** with scoped resolution
- **Multiple sources** — files, strings, or directories

---

## Features

| | |
| :--- | :--- |
| **Template engine** | Variable substitution with `{{ }}` syntax |
| **Template caching** | In-memory cache for compiled templates |
| **Context management** | Nested context with scoped variable resolution |
| **Multiple sources** | Load templates from files, strings, or directories |
| **Error handling** | Detailed error reporting for template issues |

---

## Usage

```rust
use staticweaver::{Context, Engine};
use std::time::Duration;

let engine = Engine::new("templates", Duration::from_secs(60));

let mut ctx = Context::new();
ctx.set("title".to_string(), "My Page".to_string());
ctx.set("body".to_string(), "Hello, World!".to_string());

let template = "<h1>{{title}}</h1><p>{{body}}</p>";
let output = engine.render_template(template, &ctx).unwrap();
assert_eq!(output, "<h1>My Page</h1><p>Hello, World!</p>");
```

Use [`Engine::render_page`](https://docs.rs/staticweaver) to render a `.html` file from the template directory instead of a literal string.

---

## Development

```bash
cargo build        # Build the project
cargo test         # Run all tests
cargo clippy       # Lint with Clippy
cargo fmt          # Format with rustfmt
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, signed commits, and PR guidelines.

---

**THE ARCHITECT** ᛫ [Sebastien Rousseau](https://sebastienrousseau.com)
**THE ENGINE** ᛞ [EUXIS](https://euxis.co) ᛫ Enterprise Unified Execution Intelligence System

---

## License

Dual-licensed under [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0) or [MIT](https://opensource.org/licenses/MIT), at your option.

<p align="right"><a href="#staticweaver">Back to Top</a></p>