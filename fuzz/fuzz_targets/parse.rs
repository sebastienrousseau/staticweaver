// Fuzz target: render_template must NEVER panic on arbitrary input.
// Bad templates, malformed expressions, weird Unicode, missing keys —
// all must surface as a clean EngineError, not an unwrap or arithmetic
// overflow. Issue #41 / coverage of the contract defended by
// tests/proptest_parser.rs at the property-test level.

#![no_main]

use libfuzzer_sys::fuzz_target;
use staticweaver::{Context, Engine};
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    let Ok(template) = std::str::from_utf8(data) else {
        return;
    };
    // Cap input length so the fuzzer doesn't waste its budget on
    // pathologically large strings (the engine is quadratic on some
    // workloads by design).
    if template.len() > 4096 {
        return;
    }
    let engine = Engine::new("", Duration::from_secs(60));
    let ctx = Context::new();
    // The engine must return — Ok or Err — but never panic.
    let _ = engine.render_template(template, &ctx);
});
