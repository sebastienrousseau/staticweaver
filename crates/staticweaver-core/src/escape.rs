// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Idempotent HTML escape.

use alloc::string::String;

/// Appends `s` to `out`, replacing the five HTML metacharacters with their
/// named/numeric entities. Single-quote uses the numeric `&#x27;` form so
/// the output stays valid inside both HTML and XML attributes.
///
/// Idempotent (ssg#589): already-formed entity references — `&name;`,
/// `&#NN;`, `&#xNN;` — are preserved verbatim instead of getting their
/// leading `&` re-escaped to `&amp;`. The lookup uses `scan_existing_entity`
/// with a 32-byte cap (the longest HTML5 named ref,
/// `CounterClockwiseContourIntegral`, is 31 chars).
pub(crate) fn escape_html_into(s: &str, out: &mut String) {
    // Fast-path byte scan (issue #33): the OWASP 5 metacharacters
    // (`<`, `>`, `&`, `"`, `'`) are all ASCII, so byte-indexed iteration
    // is UTF-8-safe — every multi-byte sequence has leading byte ≥ 0x80
    // and continuation bytes 0x80..=0xBF, none of which can match a
    // metacharacter byte. Safe runs flush via a single `push_str`
    // (memcpy) instead of char-by-char appends; the `matches!` arm
    // autovectorises under `lto = true, opt-level = "z"` to within
    // ~6× of the pre-ssg#589 `askama_escape` SIMD path while
    // preserving entity-awareness.
    out.reserve(s.len());
    let bytes = s.as_bytes();
    let mut start: usize = 0;
    let mut i: usize = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if !matches!(b, b'<' | b'>' | b'&' | b'"' | b'\'') {
            i += 1;
            continue;
        }
        if start < i {
            out.push_str(&s[start..i]);
        }
        match b {
            b'&' => {
                if let Some(end) = scan_existing_entity(bytes, i) {
                    // Preserve an already-formed `&name;` / `&#NN;` /
                    // `&#xNN;` verbatim — ssg#589 idempotency invariant.
                    out.push_str(&s[i..end]);
                    i = end;
                } else {
                    out.push_str("&amp;");
                    i += 1;
                }
            }
            b'<' => {
                out.push_str("&lt;");
                i += 1;
            }
            b'>' => {
                out.push_str("&gt;");
                i += 1;
            }
            b'"' => {
                out.push_str("&quot;");
                i += 1;
            }
            b'\'' => {
                out.push_str("&#x27;");
                i += 1;
            }
            _ => unreachable!(),
        }
        start = i;
    }
    if start < bytes.len() {
        out.push_str(&s[start..]);
    }
}

/// Returns `Some(end)` if `bytes[start..]` begins with a syntactically
/// valid HTML entity reference (`&name;`, `&#NN;`, or `&#xNN;`),
/// otherwise `None`. Caps the lookahead at 32 bytes — the longest
/// HTML5 named character reference (`CounterClockwiseContourIntegral`)
/// fits comfortably under that.
pub(crate) fn scan_existing_entity(
    bytes: &[u8],
    start: usize,
) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'&');
    let cap = bytes.len().min(start + 32);
    let mut j = start + 1;
    if j >= cap {
        return None;
    }
    if bytes[j] == b'#' {
        // Numeric: &#NN; or &#xNN; / &#XNN;.
        j += 1;
        if j >= cap {
            return None;
        }
        let is_hex = bytes[j] == b'x' || bytes[j] == b'X';
        if is_hex {
            j += 1;
        }
        let digits_start = j;
        while j < cap
            && ((is_hex && bytes[j].is_ascii_hexdigit())
                || (!is_hex && bytes[j].is_ascii_digit()))
        {
            j += 1;
        }
        if j == digits_start {
            return None;
        }
        if j < cap && bytes[j] == b';' {
            return Some(j + 1);
        }
        return None;
    }
    // Named: &name; where name is ASCII alphanumeric (a-z, A-Z, 0-9).
    let name_start = j;
    while j < cap && bytes[j].is_ascii_alphanumeric() {
        j += 1;
    }
    if j == name_start {
        return None;
    }
    if j < cap && bytes[j] == b';' {
        return Some(j + 1);
    }
    None
}

// ── Kani formal-verification harness (issue #43) ────────────────────
//
// Hidden behind `#[cfg(kani)]` — invisible to ordinary builds. Run with
// `cargo kani` (after `cargo install --locked kani-verifier` and
// `cargo kani-setup`). Verifies that `escape_html_into` is a fixed
// point: feeding its own output back in produces byte-identical output.
// This is the ssg#589 idempotency contract, hardened from
// proptest-defended (random seeds) to formally proven over the
// symbolic horizon Kani can solve in reasonable time.
/// A reference HTML escaper: the same contract as
/// [`escape_html_into`], written for a verifier rather than for speed.
///
/// # Why this exists
///
/// `escape_html_into` is the fast path: it scans bytes, flushes safe
/// runs with a single `push_str`, and appends into a `String`. Every one
/// of those choices is hostile to symbolic execution. Measured with Kani
/// 0.67 on a 4-byte symbolic input, proving it directly:
///
/// * `&str` slicing dominated the encoding -- `floor_char_boundary` and
///   the `slice_error_fail` path behind it accounted for over 23,000 of
///   ~20,200 loop unwindings, against 14 for this crate's own loops;
/// * removing that by slicing bytes instead made the proof tractable but
///   cost **83%** throughput (65.6 us -> 120.0 us on the
///   `escape_heavy` benchmark), because rebuilding a `&str` needs a
///   UTF-8 re-check on every push;
/// * writing into a buffer at symbolic offsets generated ~132,000
///   verification conditions and exhausted memory during propositional
///   reduction, after symbolic execution had already completed.
///
/// So the fast path stays exactly as it is, and the proofs target this
/// instead: a fixed-size, allocation-free, slice-free formulation that
/// CBMC can encode cheaply. The two are tied together by
/// `tests/escape_reference.rs`, which asserts they agree.
///
/// Returns the number of bytes written to `out`, or `None` if `out` is
/// too small.
#[cfg(any(test, kani))]
pub(crate) fn escape_html_reference(
    input: &[u8],
    out: &mut [u8],
) -> Option<usize> {
    let mut n = 0usize;
    let mut i = 0usize;

    // Appends one byte, failing if the caller's buffer is short.
    macro_rules! put {
        ($b:expr) => {{
            if n >= out.len() {
                return None;
            }
            out[n] = $b;
            n += 1;
        }};
    }
    // Appends a fixed entity.
    macro_rules! put_entity {
        ($lit:expr) => {{
            let lit: &[u8] = $lit;
            let mut k = 0usize;
            while k < lit.len() {
                put!(lit[k]);
                k += 1;
            }
        }};
    }

    while i < input.len() {
        let b = input[i];
        match b {
            b'<' => {
                put_entity!(b"&lt;");
                i += 1;
            }
            b'>' => {
                put_entity!(b"&gt;");
                i += 1;
            }
            b'"' => {
                put_entity!(b"&quot;");
                i += 1;
            }
            b'\'' => {
                put_entity!(b"&#x27;");
                i += 1;
            }
            b'&' => match scan_existing_entity(input, i) {
                // Preserve an already-formed reference verbatim -- the
                // ssg#589 idempotency invariant.
                Some(end) => {
                    while i < end {
                        put!(input[i]);
                        i += 1;
                    }
                }
                None => {
                    put_entity!(b"&amp;");
                    i += 1;
                }
            },
            other => {
                put!(other);
                i += 1;
            }
        }
    }
    Some(n)
}

#[cfg(kani)]
mod kani_proofs {
    use super::escape_html_reference;

    /// The widest escape is `&quot;` / `&#x27;` at 6 bytes per input
    /// byte, so this bounds one pass over N input bytes.
    const MAX_GROWTH: usize = 6;

    /// Input width the proofs quantify over. Four bytes covers every
    /// combination the escaper branches on, including a metacharacter
    /// adjacent to the entity text produced by escaping its neighbour --
    /// the case the idempotency contract turns on.
    const INPUT: usize = 3;

    /// Input width for the idempotency proof specifically.
    ///
    /// Narrower than `INPUT` because this harness escapes twice: the
    /// second pass takes the first pass's output as symbolic input, so
    /// its width is `IDEMPOTENT_INPUT * MAX_GROWTH`, not
    /// `IDEMPOTENT_INPUT`. At 3 that second pass is 18 bytes wide and
    /// the proof did not converge in an hour; at 2 it is 12.
    ///
    /// Two bytes still covers the case the contract turns on -- a
    /// metacharacter adjacent to the entity text produced by escaping
    /// its neighbour -- and `&x;`-style preservation is covered by the
    /// equivalence tests against the fast path.
    const IDEMPOTENT_INPUT: usize = 2;

    /// `escape(escape(x)) == escape(x)` -- the ssg#589 idempotency
    /// contract.
    ///
    /// Narrower than the single-pass proof because this harness escapes
    /// twice: the second pass takes the first pass's output as symbolic
    /// input, so its width is `IDEMPOTENT_INPUT * MAX_GROWTH` rather
    /// than `IDEMPOTENT_INPUT`. At 2 that second pass is 12 bytes wide.
    ///
    /// This harness is memory-hungry enough to be machine-dependent: it
    /// verifies on a GitHub runner but produced no verdict in 60 minutes
    /// on an 8 GB laptop, at either a 2- or 3-byte input. If it starts
    /// timing out, that is the first thing to check -- and `cargo kani
    /// --harness proof_no_bare_angle_brackets` still gives a fast signal
    /// on the cheaper property (issue #74).
    #[kani::proof]
    // Sized to the widest loop that can actually run: the verification
    // scan over INPUT * MAX_GROWTH = 18 bytes. 32 was inherited from an
    // earlier shape and unrolls every loop nearly twice as far as any
    // can execute -- and VCC count tracks unrolled program size, not
    // input width (134,633 at 3 bytes vs 134,765 at 4).
    #[kani::unwind(20)]
    fn proof_escape_is_idempotent() {
        let input: [u8; IDEMPOTENT_INPUT] = kani::any();

        let mut once = [0u8; IDEMPOTENT_INPUT * MAX_GROWTH];
        let n1 =
            escape_html_reference(&input, &mut once).expect("fits");

        let mut twice =
            [0u8; IDEMPOTENT_INPUT * MAX_GROWTH * MAX_GROWTH];
        let n2 = escape_html_reference(&once[..n1], &mut twice)
            .expect("fits");

        assert_eq!(n1, n2);
        let mut k = 0;
        while k < n1 {
            assert_eq!(once[k], twice[k]);
            k += 1;
        }
    }

    /// Output never contains a bare `<` or `>`.
    #[kani::proof]
    // Sized to the widest loop that can actually run: the verification
    // scan over INPUT * MAX_GROWTH = 18 bytes. 32 was inherited from an
    // earlier shape and unrolls every loop nearly twice as far as any
    // can execute -- and VCC count tracks unrolled program size, not
    // input width (134,633 at 3 bytes vs 134,765 at 4).
    #[kani::unwind(20)]
    fn proof_no_bare_angle_brackets() {
        let input: [u8; INPUT] = kani::any();
        let mut out = [0u8; INPUT * MAX_GROWTH];
        let n = escape_html_reference(&input, &mut out).expect("fits");

        let mut k = 0;
        while k < n {
            assert!(out[k] != b'<');
            assert!(out[k] != b'>');
            k += 1;
        }
    }
}

#[cfg(test)]
mod escape_reference_equivalence {
    //! Ties the Kani proofs to production.
    //!
    //! The proofs run against `escape_html_reference`, not against the
    //! fast path -- see that function's docs for the measurements that
    //! forced the split. A proof of the reference says nothing about
    //! `escape_html_into` unless the two are known to agree, which is
    //! what this asserts.

    use super::{escape_html_into, escape_html_reference};
    use proptest::prelude::*;

    /// Runs both escapers and returns their outputs.
    fn both(input: &str) -> (String, String) {
        let mut fast = String::new();
        escape_html_into(input, &mut fast);

        let mut buf = vec![0u8; input.len() * 6 + 16];
        let n = escape_html_reference(input.as_bytes(), &mut buf)
            .expect("reference buffer is generously sized");
        let reference = String::from_utf8(buf[..n].to_vec())
            .expect("reference emits UTF-8");

        (fast, reference)
    }

    #[test]
    fn agree_on_the_metacharacters() {
        for input in ["<", ">", "&", "\"", "'", "<&>", "a<b>c", ""] {
            let (fast, reference) = both(input);
            assert_eq!(fast, reference, "disagreement on {input:?}");
        }
    }

    #[test]
    fn agree_on_existing_entities() {
        // The idempotency invariant: an already-formed reference is
        // preserved rather than double-escaped.
        for input in [
            "&amp;",
            "&#39;",
            "&#x27;",
            "&lt;x&gt;",
            "&notanentity",
            "&;",
        ] {
            let (fast, reference) = both(input);
            assert_eq!(fast, reference, "disagreement on {input:?}");
        }
    }

    #[test]
    fn agree_on_non_ascii() {
        for input in
            ["\u{e9}", "\u{4e2d}\u{6587}", "\u{1f600}", "caf\u{e9} <b>"]
        {
            let (fast, reference) = both(input);
            assert_eq!(fast, reference, "disagreement on {input:?}");
        }
    }

    proptest! {
        /// The contract this whole split rests on.
        #[test]
        fn agree_on_arbitrary_input(input in ".{0,64}") {
            let (fast, reference) = both(&input);
            prop_assert_eq!(fast, reference);
        }

        /// Weighted towards metacharacters and entity-like runs, which
        /// arbitrary text rarely produces.
        #[test]
        fn agree_on_metacharacter_soup(
            input in proptest::collection::vec(
                proptest::sample::select(vec!['<', '>', '&', '"', '\'', ';', '#', 'x', 'a', '1']),
                0..32,
            ).prop_map(|v| v.into_iter().collect::<String>())
        ) {
            let (fast, reference) = both(&input);
            prop_assert_eq!(fast, reference);
        }
    }
}
