// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tokenizer and recursive-descent parser for template expressions.

use crate::context::Value;
use crate::error::EngineError;
use crate::expr::{CmpOp, Expr, MathOp};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
};

/// Tokenizer + parser entry point. Walks `s` once; whitespace
/// separates tokens but is otherwise insignificant.
pub(crate) fn parse_expr(s: &str) -> Result<Expr, EngineError> {
    let mut tokens = ExprTokens::new(s);
    let expr = parse_bool_or(&mut tokens)?;
    if let Some(extra) = tokens.peek() {
        return Err(EngineError::InvalidTemplate(format!(
            "unexpected token in expression: {extra:?}"
        )));
    }
    Ok(expr)
}

pub(crate) fn parse_bool_or(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_bool_and(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::Or)) {
        let _ = tokens.next();
        let rhs = parse_bool_and(tokens)?;
        lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

pub(crate) fn parse_bool_and(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_bool_not(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::And)) {
        let _ = tokens.next();
        let rhs = parse_bool_not(tokens)?;
        lhs = Expr::And(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

pub(crate) fn parse_bool_not(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    if matches!(tokens.peek(), Some(ExprTok::Not)) {
        let _ = tokens.next();
        let inner = parse_bool_not(tokens)?;
        Ok(Expr::Not(Box::new(inner)))
    } else {
        parse_test_expr(tokens)
    }
}

pub(crate) fn parse_test_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let lhs = parse_comparison(tokens)?;
    if !matches!(tokens.peek(), Some(ExprTok::Is)) {
        return Ok(lhs);
    }
    let _ = tokens.next(); // consume `is`
    let negated = matches!(tokens.peek(), Some(ExprTok::Not));
    if negated {
        let _ = tokens.next(); // consume `not`
    }
    let name = match tokens.next() {
        // All names defer to eval-time so a custom test can
        // override a built-in of the same name (matches the
        // add_filter behaviour). Built-in dispatch lives in eval.
        Some(ExprTok::Path(name)) => name,
        Some(other) => {
            return Err(EngineError::InvalidTemplate(format!(
                "expected test name after `is`, got {other:?}"
            )));
        }
        None => {
            return Err(EngineError::InvalidTemplate(
                "expected test name after `is`".to_string(),
            ));
        }
    };
    Ok(Expr::Test(Box::new(lhs), name, negated))
}

pub(crate) fn parse_comparison(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let lhs = parse_concat_expr(tokens)?;
    let op = match tokens.peek() {
        Some(ExprTok::Op(op)) => Some(op.clone()),
        _ => None,
    };
    if let Some(op) = op {
        let _ = tokens.next();
        let rhs = parse_concat_expr(tokens)?;
        Ok(Expr::Compare(Box::new(lhs), op, Box::new(rhs)))
    } else {
        Ok(lhs)
    }
}

pub(crate) fn parse_concat_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_add_expr(tokens)?;
    while matches!(tokens.peek(), Some(ExprTok::Tilde)) {
        let _ = tokens.next();
        let rhs = parse_add_expr(tokens)?;
        lhs = Expr::Concat(Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

pub(crate) fn parse_add_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_mul_expr(tokens)?;
    loop {
        let op = match tokens.peek() {
            Some(ExprTok::Plus) => MathOp::Add,
            Some(ExprTok::Minus) => MathOp::Sub,
            _ => break,
        };
        let _ = tokens.next();
        let rhs = parse_mul_expr(tokens)?;
        lhs = Expr::Math(Box::new(lhs), op, Box::new(rhs));
    }
    Ok(lhs)
}

pub(crate) fn parse_mul_expr(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    let mut lhs = parse_operand(tokens)?;
    loop {
        let op = match tokens.peek() {
            Some(ExprTok::Star) => MathOp::Mul,
            Some(ExprTok::Slash) => MathOp::Div,
            _ => break,
        };
        let _ = tokens.next();
        let rhs = parse_operand(tokens)?;
        lhs = Expr::Math(Box::new(lhs), op, Box::new(rhs));
    }
    Ok(lhs)
}

pub(crate) fn parse_operand(
    tokens: &mut ExprTokens<'_>,
) -> Result<Expr, EngineError> {
    match tokens.next() {
        Some(ExprTok::Path(p)) => Ok(Expr::Path(p)),
        Some(ExprTok::Number(n)) => Ok(Expr::Literal(Value::Number(n))),
        Some(ExprTok::String(s)) => Ok(Expr::Literal(Value::String(s))),
        Some(ExprTok::True) => Ok(Expr::Literal(Value::Bool(true))),
        Some(ExprTok::False) => Ok(Expr::Literal(Value::Bool(false))),
        Some(ExprTok::Null) => Ok(Expr::Literal(Value::Null)),
        Some(other) => Err(EngineError::InvalidTemplate(format!(
            "expected operand, got {other:?}"
        ))),
        None => Err(EngineError::InvalidTemplate(
            "expected operand, got end of expression".to_string(),
        )),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ExprTok {
    Path(String),
    Number(i64),
    String(String),
    True,
    False,
    Null,
    Op(CmpOp),
    And,
    Or,
    Not,
    Is,
    Plus,
    Minus,
    Star,
    Slash,
    /// `~` — string concat (Tera/Twig-style).
    Tilde,
}

/// Single-pass tokenizer. Tokens are produced lazily via `next` /
/// `peek`; we cache one token of lookahead so the parser stays
/// straight-line. Errors surface as `InvalidTemplate` immediately on
/// the offending byte rather than waiting until parse-time.
///
/// `prev_was_operand` disambiguates `-`: when the previous emitted
/// token was an operand (path, literal, closing of a value), `-` is a
/// binary `Minus` operator; otherwise it starts a negative number
/// literal. This lets `5 - 3` and `-3` both parse correctly.
pub(crate) struct ExprTokens<'a> {
    bytes: &'a [u8],
    pos: usize,
    peeked: Option<ExprTok>,
    prev_was_operand: bool,
}

impl<'a> ExprTokens<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
            peeked: None,
            prev_was_operand: false,
        }
    }

    fn peek(&mut self) -> Option<&ExprTok> {
        if self.peeked.is_none() {
            // Capture the current operand-state so the lookahead
            // reflects the parser's true position. peek does not
            // commit, so we must restore on next() emit.
            let saved = self.prev_was_operand;
            self.peeked = self.scan_one();
            self.prev_was_operand = saved;
        }
        self.peeked.as_ref()
    }

    fn next(&mut self) -> Option<ExprTok> {
        let tok = if let Some(tok) = self.peeked.take() {
            Some(tok)
        } else {
            self.scan_one()
        };
        if let Some(t) = &tok {
            self.prev_was_operand = is_operand_tok(t);
        }
        tok
    }

    fn scan_one(&mut self) -> Option<ExprTok> {
        // Skip whitespace.
        while self.pos < self.bytes.len()
            && self.bytes[self.pos].is_ascii_whitespace()
        {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        // Two-character comparison operators come first so they win
        // over the single-char prefix check below.
        if self.pos + 1 < self.bytes.len() {
            let two = &self.bytes[self.pos..self.pos + 2];
            let op = match two {
                b"==" => Some(CmpOp::Eq),
                b"!=" => Some(CmpOp::Ne),
                b"<=" => Some(CmpOp::Le),
                b">=" => Some(CmpOp::Ge),
                _ => None,
            };
            if let Some(op) = op {
                self.pos += 2;
                return Some(ExprTok::Op(op));
            }
        }
        // Single-char comparison operators.
        match b {
            b'<' => {
                self.pos += 1;
                return Some(ExprTok::Op(CmpOp::Lt));
            }
            b'>' => {
                self.pos += 1;
                return Some(ExprTok::Op(CmpOp::Gt));
            }
            _ => {}
        }
        // Math operators. `-` is binary only when an operand just
        // closed; otherwise it's the sign on a numeric literal.
        match b {
            b'+' => {
                self.pos += 1;
                return Some(ExprTok::Plus);
            }
            b'*' => {
                self.pos += 1;
                return Some(ExprTok::Star);
            }
            b'/' => {
                self.pos += 1;
                return Some(ExprTok::Slash);
            }
            b'-' if self.prev_was_operand => {
                self.pos += 1;
                return Some(ExprTok::Minus);
            }
            // `~` is the string-concat operator. Always binary;
            // never overloaded as a unary prefix, so no
            // prev_was_operand check.
            b'~' => {
                self.pos += 1;
                return Some(ExprTok::Tilde);
            }
            _ => {}
        }
        // String literal.
        if b == b'"' || b == b'\'' {
            return self.scan_string(b);
        }
        // Number literal (optionally signed).
        if b == b'-' || b.is_ascii_digit() {
            return self.scan_number();
        }
        // Path / keyword.
        if is_ident_start(b) {
            return self.scan_path_or_keyword();
        }
        // Unknown byte — let the parser flag it via a None / consume
        // loop. Advance so we don't spin forever.
        self.pos += 1;
        None
    }

    fn scan_string(&mut self, quote: u8) -> Option<ExprTok> {
        self.pos += 1; // consume opening quote
        let start = self.pos;
        while self.pos < self.bytes.len()
            && self.bytes[self.pos] != quote
        {
            self.pos += 1;
        }
        let raw = &self.bytes[start..self.pos];
        // Skip closing quote (or accept end-of-input as unterminated).
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
        Some(ExprTok::String(String::from_utf8_lossy(raw).into_owned()))
    }

    fn scan_number(&mut self) -> Option<ExprTok> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.bytes.len()
            && self.bytes[self.pos].is_ascii_digit()
        {
            self.pos += 1;
        }
        let raw = core::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .parse::<i64>()
            .ok()?;
        Some(ExprTok::Number(raw))
    }

    fn scan_path_or_keyword(&mut self) -> Option<ExprTok> {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && is_ident_continue(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        let raw = core::str::from_utf8(&self.bytes[start..self.pos])
            .ok()?
            .to_string();
        Some(match raw.as_str() {
            "true" => ExprTok::True,
            "false" => ExprTok::False,
            "null" => ExprTok::Null,
            "and" => ExprTok::And,
            "or" => ExprTok::Or,
            "not" => ExprTok::Not,
            "is" => ExprTok::Is,
            _ => ExprTok::Path(raw),
        })
    }
}

pub(crate) fn is_ident_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'@')
}

pub(crate) fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || matches!(b, b'0'..=b'9' | b'.')
}

/// Whether `tok` is an operand-shaped token. Used by the tokenizer to
/// decide if `-` should start a negative number literal (when the
/// previous token was *not* an operand) or be a binary subtraction
/// (when it was). Keywords like `true`/`false`/`null` count too —
/// they're literals.
pub(crate) fn is_operand_tok(tok: &ExprTok) -> bool {
    matches!(
        tok,
        ExprTok::Path(_)
            | ExprTok::Number(_)
            | ExprTok::String(_)
            | ExprTok::True
            | ExprTok::False
            | ExprTok::Null
    )
}

// ─── End expression module ─────────────────────────────────────────
