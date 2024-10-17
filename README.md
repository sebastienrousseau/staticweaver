<!-- markdownlint-disable MD033 MD041 -->
<img src="https://kura.pro/staticweaver/images/logos/staticweaver.svg"
alt="StaticWeaver logo" height="66" align="right" />
<!-- markdownlint-enable MD033 MD041 -->

# `StaticWeaver`

A fast and flexible templating engine for Rust applications.

<!-- markdownlint-disable MD033 MD041 -->
<center>
<!-- markdownlint-enable MD033 MD041 -->

[![Made With Love][made-with-rust]][08] [![Crates.io][crates-badge]][03] [![lib.rs][libs-badge]][01] [![Docs.rs][docs-badge]][04] [![Codecov][codecov-badge]][06] [![Build Status][build-badge]][07] [![GitHub][github-badge]][09]

• [Website][00] • [Documentation][04] • [Report Bug][02] • [Request Feature][02] • [Contributing Guidelines][05]

<!-- markdownlint-disable MD033 MD041 -->
</center>
<!-- markdownlint-enable MD033 MD041 -->

## Overview

`staticweaver` is a robust Rust library that provides a flexible and powerful templating engine. Designed for static site generation and more, it offers advanced caching, remote template support, and customizable rendering for optimized performance.

## Features

- **Flexible Template Rendering:** Ideal for static sites, web apps, and other use cases.
- **Dynamic Content:** Easily interpolate variables in templates with a powerful context system.
- **File and String Templates:** Render templates from both files and strings.
- **Advanced Caching:** Improve performance by caching templates for repeated use.
- **Custom Rendering:** Modify and extend the rendering process to fit your needs.
- **Remote Template Support:** Fetch and render templates from URLs.
- **Comprehensive Error Handling:** Gracefully manage template rendering errors.

## Installation

Add `staticweaver` to your `Cargo.toml`:

```toml
[dependencies]
staticweaver = "0.0.2"
```

## Usage

Here's a basic example of how to use `staticweaver`:

```rust
use std::fs;
use staticweaver::engine::Engine;
use staticweaver::EngineError;
use staticweaver::error::TemplateError;
use staticweaver::context::Context;
use std::time::Duration;
use std::path::Path;

fn main() -> Result<(), TemplateError> {
    // Create the 'examples' directory and 'template.html' for the test
    fs::create_dir_all("examples")?;
    fs::write("examples/template.html", r#"
    <!DOCTYPE html>
    <html>
    <head><title>{{title}}</title></head>
    <body><h1>{{title}}</h1><p>{{content}}</p></body>
    </html>"#)?;

    // Create a new engine with a template path and cache duration
    let mut engine = Engine::new("examples", Duration::from_secs(60));

    // Create a context with some variables
    let mut context = Context::new();
    context.set("title".to_string(), "Welcome to StaticWeaver".to_string());
    context.set("content".to_string(), "This is a simple example.".to_string());

    // Render the 'template.html', mapping EngineError to TemplateError
    let rendered = engine
        .render_page(&context, "template")
        .map_err(|e| TemplateError::EngineError(Box::new(EngineError::Render(format!("Rendering failed: {:?}", e)))))?;

    // Output the rendered content to 'rendered.html'
    fs::write("examples/rendered.html", &rendered)?;

    println!("Rendered content written to 'examples/rendered.html'.");

    // Clean up: remove files and directory
    remove_dir_contents("examples")?;

    println!("Template and rendered files cleaned up.");

    Ok(())
}

// Helper function to remove all files inside a directory
fn remove_dir_contents(dir: &str) -> std::io::Result<()> {
    let path = Path::new(dir);
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_file() {
                fs::remove_file(entry_path)?;
            } else if entry_path.is_dir() {
                fs::remove_dir_all(entry_path)?; // If there are subdirectories, remove them recursively
            }
        }
        fs::remove_dir(path)?; // Finally, remove the directory itself
    }
    Ok(())
}
```

This example demonstrates rendering a template named "page", replacing `{{title}}` and `{{content}}` with values from the context.

## Documentation

For full API documentation, please visit [docs.rs/staticweaver][04].

## Examples

To explore more examples, clone the repository and run the following command:

```shell
cargo run --example example_name
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under either of

- [Apache License, Version 2.0][10]
- [MIT license][11]

at your option.

## Acknowledgements

Special thanks to all contributors who have helped build the `staticweaver` library.

[00]: https://staticweaver.com
[01]: https://lib.rs/crates/staticweaver
[02]: https://github.com/sebastienrousseau/staticweaver/issues
[03]: https://crates.io/crates/staticweaver
[04]: https://docs.rs/staticweaver
[05]: https://github.com/sebastienrousseau/staticweaver/blob/main/CONTRIBUTING.md
[06]: https://codecov.io/gh/sebastienrousseau/staticweaver
[07]: https://github.com/sebastienrousseau/staticweaver/actions?query=branch%3Amain
[08]: https://www.rust-lang.org/
[09]: https://github.com/sebastienrousseau/staticweaver
[10]: https://www.apache.org/licenses/LICENSE-2.0
[11]: https://opensource.org/licenses/MIT

[build-badge]: https://img.shields.io/github/actions/workflow/status/sebastienrousseau/staticweaver/release.yml?branch=main&style=for-the-badge&logo=github
[codecov-badge]: https://img.shields.io/codecov/c/github/sebastienrousseau/staticweaver?style=for-the-badge&token=psbZ8MASWj&logo=codecov
[crates-badge]: https://img.shields.io/crates/v/staticweaver.svg?style=for-the-badge&color=fc8d62&logo=rust
[docs-badge]: https://img.shields.io/badge/docs.rs-staticweaver-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs
[github-badge]: https://img.shields.io/badge/github-sebastienrousseau/staticweaver-8da0cb?style=for-the-badge&labelColor=555555&logo=github
[libs-badge]: https://img.shields.io/badge/lib.rs-v0.0.2-orange.svg?style=for-the-badge
[made-with-rust]: https://img.shields.io/badge/rust-f04041?style=for-the-badge&labelColor=c0282d&logo=rust
