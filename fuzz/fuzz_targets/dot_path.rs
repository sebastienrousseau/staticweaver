// Fuzz target: dot-notation path resolution must never panic on
// arbitrary path syntax against an arbitrary Context. Numeric segments
// (`items.0`), missing keys, type mismatches, deeply nested paths,
// pathological Unicode — all must return None / clean EngineError
// rather than crashing. Issue #41.

#![no_main]

use libfuzzer_sys::fuzz_target;
use staticweaver::context::Value;
use staticweaver::{Context, Engine};
use std::time::Duration;

// Split a single input into (template_path, context_keys) so the
// fuzzer can exercise both the path-parsing side and the
// context-resolution side from one seed.
fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 4096 {
        return;
    }
    // First byte picks a split point; rest forms the path + value.
    let split = (data[0] as usize) % data.len().max(1);
    let (raw_path, raw_value) = data[1..].split_at(split.min(data.len() - 1));
    let Ok(path) = std::str::from_utf8(raw_path) else {
        return;
    };
    let Ok(value) = std::str::from_utf8(raw_value) else {
        return;
    };

    // Seed the context with a deterministic nested structure plus the
    // fuzzer-controlled value at a known key. This way many fuzzer
    // inputs trigger both the "found" and "not found" paths.
    let mut ctx = Context::new();
    ctx.set("k".to_string(), value.to_string());
    ctx.set_value("count".to_string(), 42i64);
    ctx.set_value("flag".to_string(), true);
    ctx.set_value(
        "items".to_string(),
        Value::List(vec![
            Value::from("alpha"),
            Value::from("beta"),
            Value::Number(3),
        ]),
    );

    // Context::get_path is the API contract.
    let _ = ctx.get_path(path);

    // Also exercise the rendering path so the template parser's
    // dot-path resolution is fuzzed end-to-end.
    let engine = Engine::new("", Duration::from_secs(60));
    let template = format!("{{{{{path}}}}}");
    let _ = engine.render_template(&template, &ctx);
});
