// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tag-level syntax helpers: block extraction, inheritance,
//! partial parameters, whitespace control and error positions.

use crate::context::Value;
use crate::error::EngineError;
use crate::render::BlockOverrides;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

/// Locates the body and the byte index following the matching closer for
/// a `#if` / `#each` block. `template` is positioned immediately after the
/// opener — i.e. the first character of the body.
///
/// Walks the template counting block depth so that nested `#if` inside
/// `#each` (and vice versa) match correctly. Returns `(body, after_close)`
/// where `body` is the text between the opener and the matching closer
/// and `after_close` is the substring beginning immediately after the
/// closing tag.
pub(crate) fn extract_block<'a>(
    template: &'a str,
    block: &str,
    open: &str,
    close: &str,
) -> Result<(&'a str, &'a str), EngineError> {
    let mut depth: usize = 1;
    let mut cursor = 0usize;
    while let Some(rel) = template[cursor..].find(open) {
        let abs = cursor + rel;
        let after_open = &template[abs + open.len()..];
        let end = after_open.find(close).ok_or_else(|| {
            EngineError::InvalidTemplate(
                "Unclosed template tag".to_string(),
            )
        })?;
        // Parse whitespace-control flags so `{{- /if -}}` matches `/if`
        // and trims body / after-block whitespace accordingly.
        let (inner, strip_l, strip_r) =
            parse_ws_control(after_open[..end].trim());
        let tag_end = abs + open.len() + end + close.len();

        if inner.starts_with("#if")
            || inner.starts_with("#each")
            || inner.starts_with("#block")
        {
            depth += 1;
        // Avoid `inner == format!("/{block}")` which would allocate
        // a String on every tag scan. The strip_prefix + equality
        // comparison is allocation-free and clippy-clean.
        } else if inner.strip_prefix('/') == Some(block) {
            depth -= 1;
            if depth == 0 {
                let body_raw = &template[..abs];
                let body = if strip_l {
                    body_raw.trim_end()
                } else {
                    body_raw
                };
                let after_raw = &template[tag_end..];
                let after = if strip_r {
                    after_raw.trim_start()
                } else {
                    after_raw
                };
                return Ok((body, after));
            }
        } else if inner.starts_with("/if")
            || inner.starts_with("/each")
            || inner.starts_with("/block")
        {
            // Closer for a different block type — must come from an
            // inner depth, decrement accordingly.
            depth -= 1;
        }
        cursor = tag_end;
    }
    Err(EngineError::InvalidTemplate(format!(
        "Unclosed `{{{{#{block}}}}}` block"
    )))
}

/// Parses a single filter spec from the right-hand side of a `|` chain.
/// Accepts `name`, `name:arg`, or `name:arg1,arg2,...`. Arguments may be
/// quoted with single or double quotes (so commas can appear inside an
/// arg). Returns `(name, args)` with surrounding whitespace removed.
/// Returns the (1-based) line and column of `slice` within `origin`,
/// or `None` if `slice` is not a substring of `origin` (which would
/// happen for partials loaded from a different file). Implemented
/// via pointer arithmetic — works because every &str the renderer
/// produces is sliced from the original template, never freshly
/// allocated.
pub(crate) fn position_in(
    origin: &str,
    slice: &str,
) -> Option<(usize, usize)> {
    let o_start = origin.as_ptr() as usize;
    let s_start = slice.as_ptr() as usize;
    let o_end = o_start.checked_add(origin.len())?;
    if s_start < o_start || s_start > o_end {
        return None;
    }
    let byte_pos = s_start - o_start;
    let head = origin.get(..byte_pos)?;
    let line = head.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match head.rfind('\n') {
        Some(p) => byte_pos - p,
        None => byte_pos + 1,
    };
    Some((line, col))
}

/// Returns ` at line N, column M` for a slice that lives inside
/// `origin`, or the empty string if the slice is unrelated. Append
/// to user-facing error messages — the caller sees a useful pointer
/// when the renderer can compute one, and gets the legacy bare
/// message when it can't (cross-file partials, synthetic strings).
pub(crate) fn pos_suffix(origin: &str, slice: &str) -> String {
    match position_in(origin, slice) {
        Some((line, col)) => {
            format!(" at line {line}, column {col}")
        }
        None => String::new(),
    }
}

/// Wraps an error produced by a helper (which has no access to
/// `origin`) with the position of the slice the helper was working
/// on. Idempotent: if the message already carries position info
/// from a deeper layer, it is left alone.
pub(crate) fn annotate_pos(
    err: EngineError,
    origin: &str,
    slice: &str,
) -> EngineError {
    let suffix = pos_suffix(origin, slice);
    if suffix.is_empty() {
        return err;
    }
    match err {
        EngineError::InvalidTemplate(msg)
            if !msg.contains(" at line ") =>
        {
            EngineError::InvalidTemplate(format!("{msg}{suffix}"))
        }
        EngineError::Render(msg) if !msg.contains(" at line ") => {
            EngineError::Render(format!("{msg}{suffix}"))
        }
        other => other,
    }
}

/// Parses a `name = literal` assignment used by `{{#set}}`. The literal
/// follows the same grammar as a partial parameter value: quoted string,
/// integer, bool, null, or bareword (treated as a literal string).
pub(crate) fn parse_set_assignment(
    s: &str,
) -> Result<(String, Value), EngineError> {
    let s = s.trim();
    let eq = s.find('=').ok_or_else(|| {
        EngineError::InvalidTemplate(
            "#set: missing `= value`".to_string(),
        )
    })?;
    let name = s[..eq].trim().to_string();
    if name.is_empty() {
        return Err(EngineError::InvalidTemplate(
            "#set: empty name".to_string(),
        ));
    }
    let value_str = s[eq + 1..].trim();
    if value_str.is_empty() {
        return Err(EngineError::InvalidTemplate(format!(
            "#set `{name}`: missing value"
        )));
    }
    Ok((name, parse_literal_value(value_str)))
}

/// Recognises a literal token: `"quoted"` / `'quoted'` strings,
/// `true`/`false`, `null`, integers, and bareword fallback (string).
pub(crate) fn parse_literal_value(s: &str) -> Value {
    let bytes = s.as_bytes();
    if s.len() >= 2 {
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
        {
            return Value::String(s[1..s.len() - 1].to_string());
        }
    }
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => match s.parse::<i64>() {
            Ok(n) => Value::Number(n),
            Err(_) => Value::String(s.to_string()),
        },
    }
}

/// Strips matching single or double quotes from a name token. Returns
/// the inner content unchanged if the token is unquoted.
pub(crate) fn parse_block_name(s: &str) -> Result<&str, EngineError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(EngineError::InvalidTemplate(
            "missing block name".to_string(),
        ));
    }
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
        {
            return Ok(&s[1..s.len() - 1]);
        }
    }
    Ok(s)
}

/// Returns the base-template name if `template`'s first non-whitespace
/// tag is `{{#extends "name"}}`, otherwise `None`. Quoted or bareword
/// names both work.
pub(crate) fn parse_extends<'a>(
    template: &'a str,
    open: &str,
    close: &str,
) -> Result<Option<&'a str>, EngineError> {
    let trimmed = template.trim_start();
    if !trimmed.starts_with(open) {
        return Ok(None);
    }
    let after_open = &trimmed[open.len()..];
    let end = after_open.find(close).ok_or_else(|| {
        EngineError::InvalidTemplate(
            "Unclosed template tag".to_string(),
        )
    })?;
    let inner = parse_ws_control(after_open[..end].trim()).0;
    if let Some(name_part) = inner.strip_prefix("#extends") {
        Ok(Some(parse_block_name(name_part.trim())?))
    } else {
        Ok(None)
    }
}

/// Walks a child template collecting every top-level
/// `{{#block "name"}}…{{/block}}` declaration into an owned name → body
/// map. Non-block tags (including the leading `#extends`) are skipped;
/// any literal text between blocks is silently dropped, matching the
/// Tera/Jinja convention that child templates only contribute block
/// overrides.
pub(crate) fn collect_blocks(
    template: &str,
    open: &str,
    close: &str,
) -> Result<BlockOverrides, EngineError> {
    let mut out = BlockOverrides::new();
    let mut rest = template;
    while let Some(start) = rest.find(open) {
        let after_open = &rest[start + open.len()..];
        let end = after_open.find(close).ok_or_else(|| {
            EngineError::InvalidTemplate(
                "Unclosed template tag".to_string(),
            )
        })?;
        let inner = parse_ws_control(after_open[..end].trim()).0;
        let after_tag = &after_open[end + close.len()..];

        if let Some(name_part) = inner.strip_prefix("#block") {
            let name = parse_block_name(name_part.trim())?;
            let (body, after_block) =
                extract_block(after_tag, "block", open, close)?;
            let _ = out.insert(name.to_string(), body.to_string());
            rest = after_block;
        } else {
            // Not a block opener (e.g. `#extends`, comments, literal
            // tags) — skip past it and keep scanning.
            rest = after_tag;
        }
    }
    Ok(out)
}

/// Splits a partial invocation at the first whitespace, separating the
/// partial name from its `k=v` parameter list. `name` is everything up
/// to the first space; `params` is everything after, trimmed.
///
///   "footer"            -> ("footer", "")
///   "footer year=2026"  -> ("footer", "year=2026")
pub(crate) fn split_partial_invocation(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    }
}

/// Parses a `k=v k2="v 2" k3=42 k4=true` parameter list into a vector of
/// `(name, Value)` pairs. Values may be:
///
///   - quoted strings: `"…"` or `'…'` (preserve embedded spaces)
///   - integer literals: `42`, `-7`
///   - booleans: `true`, `false`
///   - null: `null`
///   - bare identifiers: treated as literal strings
///
/// Whitespace separates pairs; unbalanced quotes return an
/// `InvalidTemplate` error.
pub(crate) fn parse_partial_params(
    s: &str,
) -> Result<Vec<(String, Value)>, EngineError> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            let _ = chars.next();
            continue;
        }
        // Read key up to '='
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            let _ = chars.next();
        }
        if key.is_empty() {
            return Err(EngineError::InvalidTemplate(
                "partial param: empty key".to_string(),
            ));
        }
        if chars.next() != Some('=') {
            return Err(EngineError::InvalidTemplate(format!(
                "partial param `{key}` missing `=value`"
            )));
        }
        // Read value: quoted run, or whitespace-terminated bareword.
        let mut value = String::new();
        match chars.peek() {
            Some(&q @ ('"' | '\'')) => {
                let _ = chars.next();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == q {
                        closed = true;
                        break;
                    }
                    value.push(c);
                }
                if !closed {
                    return Err(EngineError::InvalidTemplate(format!(
                        "partial param `{key}` has unclosed quote"
                    )));
                }
                out.push((key, Value::String(value)));
            }
            Some(_) => {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    let _ = chars.next();
                }
                let parsed = match value.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "null" => Value::Null,
                    _ => match value.parse::<i64>() {
                        Ok(n) => Value::Number(n),
                        Err(_) => Value::String(value),
                    },
                };
                out.push((key, parsed));
            }
            None => {
                return Err(EngineError::InvalidTemplate(format!(
                    "partial param `{key}` missing value"
                )));
            }
        }
    }
    Ok(out)
}

/// Parses whitespace-control dashes off a (whitespace-trimmed) tag inner.
/// Returns the inner with any dashes removed, plus two flags reporting
/// whether a left and/or right dash was present. Block comments
/// (`!-- … --`) are exempt — the closing `--` is part of their syntax.
pub(crate) fn parse_ws_control(inner: &str) -> (&str, bool, bool) {
    if inner.starts_with("!--") {
        return (inner, false, false);
    }
    let (inner, strip_l) = match inner.strip_prefix('-') {
        Some(rest) => (rest.trim_start(), true),
        None => (inner, false),
    };
    let (inner, strip_r) = match inner.strip_suffix('-') {
        Some(rest) => (rest.trim_end(), true),
        None => (inner, false),
    };
    (inner, strip_l, strip_r)
}

/// Convenience: strip whitespace-control dashes and discard the flags
/// (used by helpers that only need the cleaned inner string for matching).
pub(crate) fn strip_ws_dashes(inner: &str) -> &str {
    parse_ws_control(inner).0
}

/// Splits a `#if` body at the top-level `{{else}}`, if any. Returns
/// `(then_body, else_body)`. Nested blocks are skipped via the same depth
/// counter used by [`extract_block`].
pub(crate) fn split_else<'a>(
    body: &'a str,
    open: &str,
    close: &str,
) -> (&'a str, Option<&'a str>) {
    let mut depth: usize = 0;
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find(open) {
        let abs = cursor + rel;
        let after_open = &body[abs + open.len()..];
        let Some(end) = after_open.find(close) else {
            // Malformed — let the recursive render call surface the error.
            break;
        };
        let inner = strip_ws_dashes(after_open[..end].trim());
        let tag_end = abs + open.len() + end + close.len();

        if inner.starts_with("#if")
            || inner.starts_with("#each")
            || inner.starts_with("#block")
        {
            depth += 1;
        } else if inner.starts_with("/if")
            || inner.starts_with("/each")
            || inner.starts_with("/block")
        {
            depth = depth.saturating_sub(1);
        } else if inner == "else" && depth == 0 {
            return (&body[..abs], Some(&body[tag_end..]));
        }
        cursor = tag_end;
    }
    (body, None)
}
