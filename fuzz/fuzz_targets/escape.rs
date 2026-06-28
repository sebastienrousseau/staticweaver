// Fuzz target: html escape must be idempotent and never emit a bare
// OWASP metachar. Rendering `{{x}}` with arbitrary bytes as `x` must:
//   * never panic
//   * produce output where escape(escape(x)) == escape(x)
//   * never contain a bare `<`, `>`, or unescaped `&`
//
// Regression contract for sebastienrousseau/static-site-generator#589
// at the coverage-guided fuzz level. The same invariants are checked
// in tests/proptest_parser.rs but proptest seeds rather than mutates.

#![no_main]

use libfuzzer_sys::fuzz_target;
use staticweaver::{Context, Engine};
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = std::str::from_utf8(data) else {
        return;
    };
    if value.len() > 4096 {
        return;
    }
    let engine = Engine::new("", Duration::from_secs(60));

    let mut ctx = Context::new();
    ctx.set("x".to_string(), value.to_string());
    let Ok(once) = engine.render_template("{{x}}", &ctx) else {
        return;
    };

    // Idempotency: feeding the rendered bytes back through the engine
    // must produce the same bytes.
    let mut ctx2 = Context::new();
    ctx2.set("x".to_string(), once.clone());
    let twice = engine
        .render_template("{{x}}", &ctx2)
        .expect("second pass must also succeed if first did");
    assert_eq!(once, twice, "escape is not idempotent on input: {value:?}");

    // No bare angle brackets.
    assert!(!once.contains('<'), "bare `<` in output: {once:?}");
    assert!(!once.contains('>'), "bare `>` in output: {once:?}");

    // Every `&` must begin a valid entity terminated within 32 bytes.
    let bytes = once.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let cap = bytes.len().min(i + 33);
            let mut j = i + 1;
            while j < cap && bytes[j] != b';' {
                j += 1;
            }
            assert!(
                j < cap && bytes[j] == b';',
                "bare `&` at byte {i} in {once:?}",
            );
            i = j + 1;
        } else {
            i += 1;
        }
    }
});
