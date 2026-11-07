# staticweaver-async

Async template loading for [staticweaver](https://crates.io/crates/staticweaver):
the `AsyncTemplateLoader` trait, `MemoryAsyncLoader`, and (feature
`tokio`) `TokioFsLoader`.

Most users enable the `async` or `async-tokio` feature of the
`staticweaver` crate instead, which re-exports this crate as
`staticweaver::loader_async` and adds `Engine::render_template_async`
and friends.

Licensed under MIT or Apache-2.0, at your option.
