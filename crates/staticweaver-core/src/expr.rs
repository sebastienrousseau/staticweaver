// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Expression AST and evaluator used by `{{#if}}`, `{{#each}}`
//! ranges and postfix tests.

use crate::context::{Context, Value};
use crate::error::EngineError;
use crate::render::Renderer;
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
};

// ─── Expression module ─────────────────────────────────────────────
//
// Tiny recursive-descent grammar used by `{{#if EXPR}}` (and, in later
// phases, by other tag types). C1 added comparison operators; C2 layered
// on boolean operators; C3 layered on integer math; C4 adds the postfix
// `is <test>` predicates (`defined`, `empty`, `none`) with `is not` for
// negation:
//
//   expr       := bool_or
//   bool_or    := bool_and ( "or" bool_and )*
//   bool_and   := bool_not ( "and" bool_not )*
//   bool_not   := "not" bool_not | test_expr
//   test_expr  := comparison ( "is" "not"? TEST_NAME )?
//   comparison := concat_expr ( ("==" | "!=" | "<" | "<=" | ">" | ">=") concat_expr )?
//   concat_expr:= add_expr ( "~" add_expr )*
//   add_expr   := mul_expr ( ("+" | "-") mul_expr )*
//   mul_expr   := operand ( ("*" | "/") operand )*
//   operand    := path | literal
//   literal    := STRING | NUMBER | "true" | "false" | "null"
//   path       := IDENT ("." IDENT)*
//   TEST_NAME  := "defined" | "empty" | "none"
//
// A bare path (`{{#if user}}`) parses as a comparison with no operator
// and evaluates to the path's value, so existing `#if X` callers keep
// working without changes — `is_truthy` runs over the resulting Value.
//
// `and` / `or` / `not` / `is` are reserved when they appear as standalone
// identifiers; dotted paths (e.g. `user.notes`) are unaffected because
// the tokenizer only matches the keyword as a complete identifier. The
// test names (`defined`, `empty`, `none`) are NOT keywords — they parse
// as identifiers and the parser inspects them only after seeing `is`.
//
// Math is integer-only (the only numeric variant `Value` carries is
// `i64`). Mixed-type math (`5 + "x"`) and division by zero return
// `InvalidTemplate` errors so authors get a clear message instead of
// a panic or a silent NaN.

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub(crate) enum Expr {
    Path(String),
    Literal(Value),
    Compare(Box<Expr>, CmpOp, Box<Expr>),
    Math(Box<Expr>, MathOp, Box<Expr>),
    /// `lhs ~ rhs` — string concatenation. Both sides are coerced
    /// to their `Display` form (Number → "5", Bool → "true",
    /// Null → "", List/Map → "") and concatenated.
    Concat(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// `lhs is [not] <name>` — postfix predicate with optional
    /// negation. The name is resolved at eval time against
    /// `Engine::custom_tests` first, then the built-in set
    /// (`defined`, `empty`, `none`). Deferring resolution lets a
    /// custom test override a built-in of the same name.
    Test(Box<Expr>, String, bool),
}

impl Expr {
    /// Evaluates the expression against `ctx`, returning the result as a
    /// `Value`. Comparison expressions return `Value::Bool`; bare path
    /// expressions return whatever the lookup resolves to (or `Null`
    /// when missing). The caller decides what to do with the result —
    /// `#if` checks `is_truthy`.
    pub(crate) fn eval(
        &self,
        ctx: &Context,
        renderer: &Renderer<'_>,
    ) -> Result<Value, EngineError> {
        Ok(match self {
            Expr::Path(p) => {
                ctx.get_path(p).cloned().unwrap_or(Value::Null)
            }
            Expr::Literal(v) => v.clone(),
            Expr::Compare(lhs, op, rhs) => {
                let l = lhs.eval(ctx, renderer)?;
                let r = rhs.eval(ctx, renderer)?;
                Value::Bool(apply_cmp(op, &l, &r)?)
            }
            Expr::Math(lhs, op, rhs) => {
                let l = lhs.eval(ctx, renderer)?;
                let r = rhs.eval(ctx, renderer)?;
                Value::Number(apply_math(op, &l, &r)?)
            }
            // String concat. Both sides go through Display, so any
            // primitive (Number, Bool, String) renders sensibly;
            // Null renders as "" and List/Map render as "" too,
            // matching the substitution semantics.
            Expr::Concat(lhs, rhs) => {
                let l = lhs.eval(ctx, renderer)?;
                let r = rhs.eval(ctx, renderer)?;
                Value::String(format!("{l}{r}"))
            }
            // Boolean operators short-circuit: avoid evaluating the
            // right operand when the left already decides the result.
            // This keeps templates cheap when one side does an
            // expensive lookup or comparison.
            Expr::And(lhs, rhs) => {
                let l = lhs.eval(ctx, renderer)?;
                if l.is_truthy() {
                    Value::Bool(rhs.eval(ctx, renderer)?.is_truthy())
                } else {
                    Value::Bool(false)
                }
            }
            Expr::Or(lhs, rhs) => {
                let l = lhs.eval(ctx, renderer)?;
                if l.is_truthy() {
                    Value::Bool(true)
                } else {
                    Value::Bool(rhs.eval(ctx, renderer)?.is_truthy())
                }
            }
            Expr::Not(inner) => {
                Value::Bool(!inner.eval(ctx, renderer)?.is_truthy())
            }
            Expr::Test(operand, name, negated) => {
                // Test dispatch: custom-tests checked first so a
                // user can override a built-in of the same name.
                let result = if let Some(f) = renderer.custom_test(name)
                {
                    let value = operand.eval(ctx, renderer)?;
                    f(&value, &[])?
                } else {
                    match name.as_str() {
                        "defined" => {
                            // `defined` is special: bare path
                            // operands check for path existence
                            // without defaulting to Null. Other
                            // operand shapes reduce to "not Null".
                            if let Expr::Path(p) = operand.as_ref() {
                                ctx.get_path(p).is_some()
                            } else {
                                !matches!(
                                    operand.eval(ctx, renderer)?,
                                    Value::Null
                                )
                            }
                        }
                        "empty" => is_value_empty(
                            &operand.eval(ctx, renderer)?,
                        ),
                        "none" => matches!(
                            operand.eval(ctx, renderer)?,
                            Value::Null
                        ),
                        unknown => {
                            return Err(EngineError::InvalidTemplate(
                                format!(
                                    "unknown test `{unknown}` — \
                                         expected `defined`, `empty`, \
                                         `none`, or a name registered \
                                         via Engine::add_test"
                                ),
                            ));
                        }
                    }
                };
                Value::Bool(if *negated { !result } else { result })
            }
        })
    }
}

/// Compares two values per `op`. `Eq`/`Ne` use structural equality and
/// work on every variant pair. The ordered comparisons (`Lt`/`Le`/`Gt`
/// /`Ge`) require both operands to be numbers or both to be strings;
/// any other combination returns an `InvalidTemplate` error so authors
/// get a clear message instead of silent type coercion.
pub(crate) fn apply_cmp(
    op: &CmpOp,
    lhs: &Value,
    rhs: &Value,
) -> Result<bool, EngineError> {
    use core::cmp::Ordering;
    match op {
        CmpOp::Eq => Ok(lhs == rhs),
        CmpOp::Ne => Ok(lhs != rhs),
        _ => {
            let ord = match (lhs, rhs) {
                (Value::Number(a), Value::Number(b)) => a.cmp(b),
                (Value::String(a), Value::String(b)) => a.cmp(b),
                _ => {
                    return Err(EngineError::InvalidTemplate(format!(
                        "cannot order {lhs:?} and {rhs:?} — \
                         both operands must be numbers or both strings"
                    )));
                }
            };
            Ok(matches!(
                (op, ord),
                (CmpOp::Lt, Ordering::Less)
                    | (CmpOp::Le, Ordering::Less | Ordering::Equal)
                    | (CmpOp::Gt, Ordering::Greater)
                    | (CmpOp::Ge, Ordering::Greater | Ordering::Equal)
            ))
        }
    }
}

/// Applies an integer math op. Both operands must be `Value::Number`;
/// any other combination errors out so authors don't get silent
/// coercion. Division by zero returns an `InvalidTemplate` error
/// rather than panicking.
pub(crate) fn apply_math(
    op: &MathOp,
    lhs: &Value,
    rhs: &Value,
) -> Result<i64, EngineError> {
    let (a, b) = match (lhs, rhs) {
        (Value::Number(a), Value::Number(b)) => (*a, *b),
        _ => {
            return Err(EngineError::InvalidTemplate(format!(
                "math operator requires two numbers, got \
                 {lhs:?} and {rhs:?}"
            )));
        }
    };
    match op {
        MathOp::Add => a.checked_add(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} + {b}"
            ))
        }),
        MathOp::Sub => a.checked_sub(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} - {b}"
            ))
        }),
        MathOp::Mul => a.checked_mul(b).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "integer overflow in {a} * {b}"
            ))
        }),
        MathOp::Div => {
            if b == 0 {
                Err(EngineError::InvalidTemplate(
                    "division by zero".to_string(),
                ))
            } else {
                a.checked_div(b).ok_or_else(|| {
                    EngineError::InvalidTemplate(format!(
                        "integer overflow in {a} / {b}"
                    ))
                })
            }
        }
    }
}

/// Whether a value is "empty" for `is empty`. Strings, lists, maps
/// each have a natural empty form; `Null` is considered empty so
/// `unset is empty` is true. Numbers and bools are never empty —
/// `0 is empty` is false on purpose, matching Tera/Jinja semantics.
pub(crate) fn is_value_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::List(l) => l.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}
