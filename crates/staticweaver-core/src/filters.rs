// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Built-in filter pipeline (`{{ x | filter:arg }}`).

#[cfg(feature = "json")]
use crate::context::Value;
use crate::error::EngineError;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

pub(crate) fn parse_filter(spec: &str) -> (String, Vec<String>) {
    let spec = spec.trim();
    let (name, args_str) = match spec.find(':') {
        Some(i) => (&spec[..i], &spec[i + 1..]),
        None => (spec, ""),
    };
    let args = if args_str.is_empty() {
        Vec::new()
    } else {
        parse_filter_args(args_str)
    };
    (name.trim().to_string(), args)
}

/// Splits a filter argument list on `,`, honouring single- and
/// double-quoted spans so a quoted comma is preserved verbatim. Quotes
/// themselves are stripped from the returned argument values.
pub(crate) fn parse_filter_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quote: Option<char> = None;
    for c in s.chars() {
        match (c, in_quote) {
            ('"', None) => in_quote = Some('"'),
            ('\'', None) => in_quote = Some('\''),
            (q, Some(open)) if q == open => in_quote = None,
            (',', None) => {
                out.push(core::mem::take(&mut buf).trim().to_string());
            }
            (c, _) => buf.push(c),
        }
    }
    let last = buf.trim().to_string();
    if !last.is_empty() || !out.is_empty() {
        out.push(last);
    }
    out
}

/// Applies a single named filter to `input`. Filters that need
/// arguments parse them out of `args`; missing arguments fall back to
/// each filter's documented default. Unknown filter names are reported
/// as a `Render` error so authors get a clear pointer.
pub(crate) fn apply_filter(
    name: &str,
    args: &[String],
    input: String,
) -> Result<String, EngineError> {
    match name {
        "uppercase" => Ok(input.to_uppercase()),
        "lowercase" => Ok(input.to_lowercase()),
        "trim" => Ok(input.trim().to_string()),
        "truncate" => {
            // Default 30 chars (Unicode-aware), suffix "..." appended.
            let limit: usize =
                args.first().and_then(|s| s.parse().ok()).unwrap_or(30);
            let suffix = "...";
            let n = input.chars().count();
            if n > limit {
                let head_len =
                    limit.saturating_sub(suffix.chars().count());
                let mut head: String =
                    input.chars().take(head_len).collect();
                head.push_str(suffix);
                Ok(head)
            } else {
                Ok(input)
            }
        }
        // Capitalize: ASCII-flavoured first-letter uppercase, rest as-is.
        "capitalize" => {
            let mut chars = input.chars();
            Ok(match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars)
                    .collect::<String>(),
                None => input,
            })
        }
        // Length: Unicode character count for strings.
        "length" => Ok(input.chars().count().to_string()),
        // Default: returns the first arg when input is empty,
        // otherwise the input unchanged. `{{ name | default:"anon" }}`.
        "default" => {
            if input.is_empty() {
                Ok(args.first().cloned().unwrap_or_default())
            } else {
                Ok(input)
            }
        }
        // Replace all occurrences of arg 0 with arg 1.
        "replace" => match (args.first(), args.get(1)) {
            (Some(from), Some(to)) => Ok(input.replace(from, to)),
            _ => Err(EngineError::Render(
                "replace filter requires two args: replace:\"from\",\"to\""
                    .to_string(),
            )),
        },
        // URL-encode (RFC 3986 unreserved set is preserved). Hand-rolled
        // to avoid a `urlencoding` dep; correct for query-string use.
        "urlencode" => Ok(url_encode(&input)),
        // Mark a value as already safe — emit raw, *without* the engine's
        // HTML escape on top. This is the filter form of the `{{!key}}`
        // raw opt-out; callers rendering pre-escaped HTML use it from a
        // pipeline (`{{ snippet | safe }}`).
        //
        // Implementation note: the actual escape suppression is handled
        // by the dispatch loop checking for a trailing `safe` filter
        // (see `apply_filter_chain`). Here `safe` is a no-op pass-through
        // so the chain composes naturally.
        "safe" => Ok(input),
        // Numeric filters. Inputs are parsed as f64 so they accept
        // both i64-shaped strings ("42") and float-shaped strings
        // ("3.14"); pass-through identity for ceil/floor/round on
        // values that happen to have no fractional part.
        "abs" => parse_number_filter(&input, "abs", |n| n.abs()),
        "round" => parse_number_filter(&input, "round", crate::num::round),
        "ceil" => parse_number_filter(&input, "ceil", crate::num::ceil),
        "floor" => parse_number_filter(&input, "floor", crate::num::floor),
        "number_format" => {
            let sep =
                args.first().map(String::as_str).unwrap_or(",");
            number_format(&input, sep)
        }
        // String filters. Complement the case-folding / trimming
        // set with shape-changing transforms.
        "repeat" => {
            let n: usize = args
                .first()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    EngineError::Render(
                        "repeat filter requires a positive integer arg"
                            .to_string(),
                    )
                })?;
            Ok(input.repeat(n))
        }
        "reverse" => Ok(input.chars().rev().collect()),
        "slice" => {
            // slice:start,end — Unicode-aware character slicing.
            let start: usize = args
                .first()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    EngineError::Render(
                        "slice filter requires start as the first arg"
                            .to_string(),
                    )
                })?;
            let end: Option<usize> =
                args.get(1).and_then(|s| s.parse().ok());
            let chars: Vec<char> = input.chars().collect();
            let from = start.min(chars.len());
            let to = end.unwrap_or(chars.len()).min(chars.len());
            Ok(chars[from..to.max(from)].iter().collect())
        }
        "pad_start" => pad(&input, args, true),
        "pad_end" => pad(&input, args, false),
        // Boolean-shaped filters: emit "true" or "false". Most
        // useful for direct substitution (e.g. emitting a CSS
        // class conditionally) — for `#if EXPR` you want the
        // built-in `is X` tests or a registered custom test.
        "contains" => Ok(args
            .first()
            .map(|n| input.contains(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        "starts_with" => Ok(args
            .first()
            .map(|n| input.starts_with(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        "ends_with" => Ok(args
            .first()
            .map(|n| input.ends_with(n.as_str()).to_string())
            .unwrap_or_else(|| "false".to_string())),
        unknown => Err(EngineError::Render(format!(
            "Unknown filter: {unknown}"
        ))),
    }
}

/// Pads `s` with `pad_char` (default `' '`) up to `width`
/// characters. `left = true` pads on the left (right-align);
/// `false` pads on the right (left-align). Already-long inputs
/// are returned unchanged.
/// Serialises a `Value` as JSON via `serde_json`. Used by the
/// `json` filter (feature-gated). Required because the standard
/// filter pipeline operates on `String`, which would lose any
/// `List` / `Map` structure (those Display as the empty string).
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub(crate) fn json_encode_value(
    v: &Value,
) -> Result<String, EngineError> {
    let json = value_to_serde_json(v);
    serde_json::to_string(&json)
        .map_err(|e| EngineError::Render(format!("json filter: {e}")))
}

#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub(crate) fn value_to_serde_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Number(n) => {
            serde_json::Value::Number(serde_json::Number::from(*n))
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(
            items.iter().map(value_to_serde_json).collect(),
        ),
        Value::Map(map) => {
            let mut obj = serde_json::Map::new();
            // Sort keys for deterministic output — matches the
            // ordering the engine uses everywhere else (Map iter
            // is unspecified for FnvHashMap).
            let mut keyed: Vec<_> = map.iter().collect();
            keyed.sort_by(|a, b| a.0.cmp(b.0));
            for (k, val) in keyed {
                let _ = obj.insert(k.clone(), value_to_serde_json(val));
            }
            serde_json::Value::Object(obj)
        }
    }
}

pub(crate) fn pad(
    s: &str,
    args: &[String],
    left: bool,
) -> Result<String, EngineError> {
    let width: usize =
        args.first().and_then(|s| s.parse().ok()).ok_or_else(|| {
            EngineError::Render(
                "pad filter requires width as the first arg"
                    .to_string(),
            )
        })?;
    let pad_char =
        args.get(1).and_then(|c| c.chars().next()).unwrap_or(' ');
    let len = s.chars().count();
    if len >= width {
        return Ok(s.to_string());
    }
    let needed = width - len;
    let pad: String = core::iter::repeat_n(pad_char, needed).collect();
    Ok(if left {
        format!("{pad}{s}")
    } else {
        format!("{s}{pad}")
    })
}

/// Parses `input` as `f64`, applies `op`, formats the result. If
/// the result has no fractional part it renders as an integer
/// ("3" not "3.0"). Used by abs/round/ceil/floor.
pub(crate) fn parse_number_filter(
    input: &str,
    name: &str,
    op: impl FnOnce(f64) -> f64,
) -> Result<String, EngineError> {
    let n: f64 = input.trim().parse().map_err(|_| {
        EngineError::Render(format!(
            "{name} filter: expected a number, got `{input}`"
        ))
    })?;
    let r = op(n);
    Ok(if r.is_finite() && crate::num::fract(r) == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    })
}

/// Inserts `sep` between every group of three digits, counting from
/// the decimal point (or the end of the integer). Preserves a
/// leading `-`. Non-numeric input returns an error.
pub(crate) fn number_format(
    input: &str,
    sep: &str,
) -> Result<String, EngineError> {
    let trimmed = input.trim();
    let (sign, rest) = match trimmed.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", trimmed),
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    if int_part.is_empty()
        || !int_part.chars().all(|c| c.is_ascii_digit())
        || frac_part
            .is_some_and(|f| !f.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(EngineError::Render(format!(
            "number_format filter: expected a number, got `{input}`"
        )));
    }
    let bytes = int_part.as_bytes();
    let mut grouped = String::with_capacity(int_part.len() * 2);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push_str(sep);
        }
        grouped.push(b as char);
    }
    Ok(match frac_part {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    })
}

/// RFC 3986 percent-encoding for the unreserved set (`A-Z a-z 0-9 - _ . ~`).
/// Everything else becomes `%HH`. Hand-rolled to avoid an extra dep.
pub(crate) fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => out.push(byte as char),
            other => {
                use core::fmt::Write as _;
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}
