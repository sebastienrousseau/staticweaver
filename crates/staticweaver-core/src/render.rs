// Copyright © 2024-2026 StaticWeaver. All rights reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The render loop: [`Renderer`] walks a template, dispatches each tag
//! and writes the output. It borrows its configuration (delimiters,
//! escape policy, filters, tests, loader) so a host such as
//! `staticweaver::Engine` can build one per call without copying.

use crate::context::{Context, Map, Value};
use crate::error::EngineError;
use crate::escape::escape_html_into;
#[cfg(feature = "json")]
use crate::filters::json_encode_value;
use crate::filters::{apply_filter, parse_filter};
use crate::loader::{FilterFn, Registry, TemplateLoader, TestFn};
use crate::path::validate_name;
use crate::syntax::{
    annotate_pos, collect_blocks, extract_block, parse_block_name,
    parse_extends, parse_partial_params, parse_set_assignment,
    parse_ws_control, pos_suffix, split_else, split_partial_invocation,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

/// Owned name → body map collected from a child template's
/// `{{#block "name"}}…{{/block}}` declarations and consumed by the base
/// template's matching `{{#block "name"}}` tags. Owned strings sidestep
/// lifetime entanglement when blocks are merged across multiple
/// `{{#extends}}` levels.
pub(crate) type BlockOverrides = BTreeMap<String, String>;

/// Loop-control signal returned by the render walk. `Continue`
/// asks the enclosing `#each` to skip to the next iteration;
/// `Break` asks it to stop iterating. `Done` is the normal
/// terminal state and bubbles up unchanged through every layer.
/// At the top level the signal is silently discarded: using
/// `#break` / `#continue` outside a loop is a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowSignal {
    Done,
    Break,
    Continue,
}

/// Maximum nesting depth for `{{#extends}}`, partial inclusion, and
/// `{{#block}}` body rendering combined. Caps mutually-recursive
/// templates before the stack does.
const MAX_RENDER_DEPTH: usize = 10;

/// Per-level render state. `origin` is the source the current
/// template slice was cut from (for line:col error positions);
/// `super_body` is the parent block's default body inside a
/// `{{#block}}` override.
#[derive(Clone, Copy)]
struct Frame<'t> {
    origin: &'t str,
    blocks: &'t BlockOverrides,
    super_body: Option<&'t str>,
    depth: usize,
    escape: bool,
}

impl Frame<'_> {
    fn deeper(self) -> Self {
        Frame {
            depth: self.depth + 1,
            ..self
        }
    }
}

/// One scanned tag: its trimmed key and the text after it.
struct Tag<'t> {
    key: &'t str,
    after: &'t str,
}

/// Result of scanning up to the next opening delimiter.
enum Scan<'t> {
    /// A backslash-escaped delimiter was emitted; resume here.
    Escaped(&'t str),
    /// A real tag to dispatch.
    Tag(Tag<'t>),
}

/// What the walk does after a tag: continue at `Next`, or unwind
/// with a loop-control signal.
enum Step<'t> {
    Next(&'t str),
    Signal(FlowSignal),
}

/// Unwinds on `Break` / `Continue`, otherwise resumes at `next`.
fn propagate(signal: FlowSignal, next: &str) -> Step<'_> {
    if signal == FlowSignal::Done {
        Step::Next(next)
    } else {
        Step::Signal(signal)
    }
}

fn check_depth(depth: usize) -> Result<(), EngineError> {
    if depth > MAX_RENDER_DEPTH {
        return Err(EngineError::Render(format!(
            "Maximum template recursion depth ({MAX_RENDER_DEPTH}) exceeded"
        )));
    }
    Ok(())
}

/// The template renderer.
///
/// Holds borrowed configuration only, so it is cheap to build per
/// call. Without the `std` feature this is the whole engine: bring a
/// [`TemplateLoader`] (a `BTreeMap<String, String>` works) and render.
///
/// # Examples
///
/// ```
/// use staticweaver_core::{Context, Renderer};
/// use std::collections::BTreeMap;
///
/// let loader: BTreeMap<String, String> = BTreeMap::new();
/// let mut ctx = Context::new();
/// ctx.set("name".to_string(), "<Ada>".to_string());
/// let out = Renderer::new(&loader).render("Hi {{name}}", &ctx).unwrap();
/// assert_eq!(out, "Hi &lt;Ada&gt;");
/// ```
pub struct Renderer<'a> {
    loader: &'a dyn TemplateLoader,
    filters: Option<&'a dyn Registry<FilterFn>>,
    tests: Option<&'a dyn Registry<TestFn>>,
    open: &'a str,
    close: &'a str,
    escape_html: bool,
    lax_undefined: bool,
}

impl fmt::Debug for Renderer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("open", &self.open)
            .field("close", &self.close)
            .field("escape_html", &self.escape_html)
            .field("lax_undefined", &self.lax_undefined)
            .finish_non_exhaustive()
    }
}

impl<'a> Renderer<'a> {
    /// Creates a renderer over `loader` with the defaults: `{{` / `}}`
    /// delimiters, HTML escape on, strict undefined, no custom
    /// filters or tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let r = Renderer::new(&loader);
    /// assert_eq!(r.render("plain", &Context::new()).unwrap(), "plain");
    /// ```
    #[must_use]
    pub fn new(loader: &'a dyn TemplateLoader) -> Self {
        Self {
            loader,
            filters: None,
            tests: None,
            open: "{{",
            close: "}}",
            escape_html: true,
            lax_undefined: false,
        }
    }

    /// Sets the tag delimiters.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let mut ctx = Context::new();
    /// ctx.set("k".to_string(), "v".to_string());
    /// let r = Renderer::new(&loader).delimiters("<<", ">>");
    /// assert_eq!(r.render("<<k>>", &ctx).unwrap(), "v");
    /// ```
    #[must_use]
    pub fn delimiters(self, open: &'a str, close: &'a str) -> Self {
        Self {
            open,
            close,
            ..self
        }
    }

    /// Turns HTML escape of substitutions on or off.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let mut ctx = Context::new();
    /// ctx.set("k".to_string(), "<b>".to_string());
    /// let r = Renderer::new(&loader).escape_html(false);
    /// assert_eq!(r.render("{{k}}", &ctx).unwrap(), "<b>");
    /// ```
    #[must_use]
    pub fn escape_html(self, escape_html: bool) -> Self {
        Self {
            escape_html,
            ..self
        }
    }

    /// When `true`, an unresolved `{{key}}` renders as nothing
    /// instead of failing.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let r = Renderer::new(&loader).lax_undefined(true);
    /// assert_eq!(r.render("a{{missing}}b", &Context::new()).unwrap(), "ab");
    /// ```
    #[must_use]
    pub fn lax_undefined(self, lax_undefined: bool) -> Self {
        Self {
            lax_undefined,
            ..self
        }
    }

    /// Sets the custom filters, consulted before the built-ins.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, FilterFn, Renderer};
    /// use std::collections::BTreeMap;
    /// use std::sync::Arc;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let mut filters: BTreeMap<String, FilterFn> = BTreeMap::new();
    /// let _ = filters.insert("x2".into(), Arc::new(|s, _| Ok(s.repeat(2))));
    /// let mut ctx = Context::new();
    /// ctx.set("k".to_string(), "ab".to_string());
    /// let r = Renderer::new(&loader).filters(&filters);
    /// assert_eq!(r.render("{{ k | x2 }}", &ctx).unwrap(), "abab");
    /// ```
    #[must_use]
    pub fn filters(self, filters: &'a dyn Registry<FilterFn>) -> Self {
        Self {
            filters: Some(filters),
            ..self
        }
    }

    /// Sets the custom tests, consulted before the built-ins.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer, TestFn};
    /// use std::collections::BTreeMap;
    /// use std::sync::Arc;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let mut tests: BTreeMap<String, TestFn> = BTreeMap::new();
    /// let _ = tests.insert("always".into(), Arc::new(|_, _| Ok(true)));
    /// let r = Renderer::new(&loader).tests(&tests);
    /// let out = r.render("{{#if x is always}}y{{/if}}", &Context::new());
    /// assert_eq!(out.unwrap(), "y");
    /// ```
    #[must_use]
    pub fn tests(self, tests: &'a dyn Registry<TestFn>) -> Self {
        Self {
            tests: Some(tests),
            ..self
        }
    }

    /// Renders `template` against `context`. An empty template renders
    /// as an empty string; the facade's `Engine::render_template`
    /// rejects it instead.
    ///
    /// # Errors
    ///
    /// Returns `EngineError::InvalidTemplate` for malformed syntax,
    /// `EngineError::Render` for unresolved tags or recursion that
    /// exceeds the depth cap, and any error a loader, filter or test
    /// returns.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let r = Renderer::new(&loader);
    /// assert!(r.render("{{missing}}", &Context::new()).is_err());
    /// ```
    pub fn render(
        &self,
        template: &str,
        context: &Context,
    ) -> Result<String, EngineError> {
        let mut output = String::with_capacity(template.len());
        // Top-level FlowSignal is discarded: `#break` / `#continue`
        // outside a loop is a no-op.
        let _ = self.resolve(
            template,
            template,
            context,
            BlockOverrides::new(),
            &mut output,
            0,
        )?;
        Ok(output)
    }

    /// Validates `name`, loads it through the loader, and renders it.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`], plus `EngineError::InvalidTemplate`
    /// for a refused name and the loader's error for a missing one.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let mut loader = BTreeMap::new();
    /// let _ = loader.insert("page".to_string(), "Hi".to_string());
    /// let r = Renderer::new(&loader);
    /// assert_eq!(r.render_named("page", &Context::new()).unwrap(), "Hi");
    /// ```
    pub fn render_named(
        &self,
        name: &str,
        context: &Context,
    ) -> Result<String, EngineError> {
        validate_name(name)?;
        let body = self.loader.load(name)?;
        self.render(&body, context)
    }

    /// Renders `template` into a [`core::fmt::Write`] sink. The
    /// `std` facade adds a `std::io::Write` shim on top.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`], plus `EngineError::Render` when the
    /// sink refuses the write.
    ///
    /// # Examples
    ///
    /// ```
    /// use staticweaver_core::{Context, Renderer};
    /// use std::collections::BTreeMap;
    ///
    /// let loader: BTreeMap<String, String> = BTreeMap::new();
    /// let mut out = String::new();
    /// Renderer::new(&loader).render_to("x", &Context::new(), &mut out).unwrap();
    /// assert_eq!(out, "x");
    /// ```
    pub fn render_to<W: fmt::Write>(
        &self,
        template: &str,
        context: &Context,
        writer: &mut W,
    ) -> Result<(), EngineError> {
        let rendered = self.render(template, context)?;
        writer.write_str(&rendered).map_err(|_| {
            EngineError::Render(
                "write to output sink failed".to_string(),
            )
        })
    }

    /// Looks up a custom test by name (used by the expression
    /// evaluator).
    pub(crate) fn custom_test(&self, name: &str) -> Option<&TestFn> {
        self.tests.and_then(|t| t.lookup(name))
    }

    /// Resolves any `{{#extends "base"}}` chain on `template` before
    /// rendering. Each level's `{{#block "name"}}…{{/block}}`
    /// declarations are collected and merged into `accumulated`; child
    /// definitions win over parent (or_insert preserves existing).
    /// Once a template that does not extend anything is reached, the
    /// fully-merged overrides are handed to `walk` for the real render.
    fn resolve(
        &self,
        origin: &str,
        template: &str,
        context: &Context,
        mut accumulated: BlockOverrides,
        output: &mut String,
        depth: usize,
    ) -> Result<FlowSignal, EngineError> {
        check_depth(depth)?;
        let Some(base_name) =
            parse_extends(template, self.open, self.close)?
        else {
            let frame = Frame {
                origin,
                blocks: &accumulated,
                super_body: None,
                depth,
                escape: self.escape_html,
            };
            return self.walk(frame, template, context, output);
        };
        validate_name(base_name)?;
        for (k, v) in collect_blocks(template, self.open, self.close)? {
            let _ = accumulated.entry(k).or_insert(v);
        }
        let base_content = self.loader.load(base_name)?;
        // Switch `origin` to the base file's content: line numbers in
        // errors reported during base-template rendering refer to the
        // base file, not the child.
        self.resolve(
            &base_content,
            &base_content,
            context,
            accumulated,
            output,
            depth + 1,
        )
    }

    /// Renders `template` against `context`, appending to `output`.
    ///
    /// Block bodies are rendered through this same function, so
    /// escaping, dot-notation, and nested control flow compose without
    /// duplication. `{{#set k = v}}` writes to a local scope that is
    /// materialised lazily on the first set; recursive descent into
    /// `#if` / `#each` / `#block` / partial bodies inherits whatever the
    /// caller's scope had at that moment.
    fn walk<'t>(
        &self,
        frame: Frame<'t>,
        template: &'t str,
        context: &Context,
        output: &mut String,
    ) -> Result<FlowSignal, EngineError> {
        check_depth(frame.depth)?;
        let mut rest = template;
        let mut local: Option<Context> = None;
        while let Some(start) = rest.find(self.open) {
            let step =
                match self.scan(frame.origin, rest, start, output)? {
                    Scan::Escaped(next) => Step::Next(next),
                    Scan::Tag(tag) => self.step(
                        frame, &tag, &mut local, context, output,
                    )?,
                };
            match step {
                Step::Next(next) => rest = next,
                Step::Signal(signal) => return Ok(signal),
            }
        }
        output.push_str(rest);
        Ok(FlowSignal::Done)
    }

    /// Handles one tag: `{{#set}}` updates the local scope, everything
    /// else renders against it (or the parent context before any set).
    fn step<'t>(
        &self,
        frame: Frame<'t>,
        tag: &Tag<'t>,
        local: &mut Option<Context>,
        context: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        if let Some(assignment) = tag.key.strip_prefix("#set") {
            assign(frame.origin, tag, assignment, local, context)?;
            return Ok(Step::Next(tag.after));
        }
        let active = local.as_ref().unwrap_or(context);
        self.dispatch(frame, tag, active, context, output)
    }

    /// Copies the text before the next tag to `output` and parses the
    /// tag. A run of backslashes before the delimiter is halved; an odd
    /// run escapes the delimiter, which is then emitted literally.
    fn scan<'t>(
        &self,
        origin: &str,
        rest: &'t str,
        start: usize,
        output: &mut String,
    ) -> Result<Scan<'t>, EngineError> {
        let after_open = &rest[start + self.open.len()..];
        if copy_text(rest, start, output) {
            output.push_str(self.open);
            return Ok(Scan::Escaped(after_open));
        }
        let end = after_open.find(self.close).ok_or_else(|| {
            EngineError::InvalidTemplate(format!(
                "Unclosed template tag{}",
                pos_suffix(origin, &rest[start..])
            ))
        })?;
        let key_raw = &after_open[..end];
        if key_raw.contains(self.open) {
            return Err(EngineError::InvalidTemplate(format!(
                "Nested delimiters are not allowed{}",
                pos_suffix(origin, key_raw)
            )));
        }
        let after_raw = &after_open[end + self.close.len()..];
        Ok(Scan::Tag(trim_tag(key_raw, after_raw, output)))
    }

    /// Routes one tag to its handler.
    fn dispatch<'t>(
        &self,
        frame: Frame<'t>,
        tag: &Tag<'t>,
        active: &Context,
        context: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let key = tag.key;
        if let Some(arg) = key.strip_prefix("#if") {
            return self.tag_if(frame, arg.trim(), tag, active, output);
        }
        if let Some(arg) = key.strip_prefix("#each") {
            return self.tag_each(
                frame,
                arg.trim(),
                tag,
                active,
                output,
            );
        }
        if let Some(name) = key.strip_prefix("#block") {
            return self.tag_block(
                frame,
                name.trim(),
                tag,
                context,
                output,
            );
        }
        if let Some(call) = key.strip_prefix('>') {
            return self.tag_partial(
                frame,
                call.trim(),
                tag,
                active,
                output,
            );
        }
        self.dispatch_inline(frame, tag, active, context, output)
    }

    /// Tags without a body: loop control, `super()`, comments and
    /// substitutions.
    fn dispatch_inline<'t>(
        &self,
        frame: Frame<'t>,
        tag: &Tag<'t>,
        active: &Context,
        context: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        match tag.key {
            "#break" => Ok(Step::Signal(FlowSignal::Break)),
            "#continue" => Ok(Step::Signal(FlowSignal::Continue)),
            "super()" => {
                self.tag_super(frame, tag.after, context, output)
            }
            key if is_comment(key) => Ok(Step::Next(tag.after)),
            key => {
                reject_stray_closer(frame.origin, key)?;
                self.tag_substitute(frame, tag, active, output)
            }
        }
    }

    /// Parses and evaluates `src`, stamping errors with the position
    /// of `anchor`.
    fn eval_expr(
        &self,
        origin: &str,
        src: &str,
        anchor: &str,
        ctx: &Context,
    ) -> Result<Value, EngineError> {
        crate::expr_parse::parse_expr(src)
            .map_err(|e| annotate_pos(e, origin, anchor))?
            .eval(ctx, self)
            .map_err(|e| annotate_pos(e, origin, anchor))
    }

    /// `{{#if EXPR}}…{{else}}…{{/if}}`. A bare path keeps its
    /// truthiness semantics; a comparison evaluates to Bool.
    fn tag_if<'t>(
        &self,
        frame: Frame<'t>,
        arg: &str,
        tag: &Tag<'t>,
        active: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let (body, after_block) =
            extract_block(tag.after, "if", self.open, self.close)
                .map_err(|e| annotate_pos(e, frame.origin, tag.key))?;
        let (then_body, else_body) =
            split_else(body, self.open, self.close);
        let cond =
            self.eval_expr(frame.origin, arg, arg, active)?.is_truthy();
        let chosen = if cond {
            then_body
        } else {
            else_body.unwrap_or("")
        };
        if chosen.is_empty() {
            return Ok(Step::Next(after_block));
        }
        // Break/Continue propagate upward; the enclosing #each (if
        // any) handles it.
        let signal =
            self.walk(frame.deeper(), chosen, active, output)?;
        Ok(propagate(signal, after_block))
    }

    /// `{{#each list}}`, `{{#each map}}` (sorted by key, binds `@key`)
    /// and `{{#each START..END}}` (END exclusive).
    fn tag_each<'t>(
        &self,
        frame: Frame<'t>,
        arg: &str,
        tag: &Tag<'t>,
        active: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let (body, after_block) =
            extract_block(tag.after, "each", self.open, self.close)
                .map_err(|e| annotate_pos(e, frame.origin, tag.key))?;
        let range = self.each_range(frame.origin, arg, active)?;
        let entries =
            each_entries(frame.origin, arg, active, range.as_deref())?;
        // Clone the active context ONCE, then overwrite the loop
        // variables in place each iteration (nested `#set` writes to a
        // local scope and does not leak back into `child`).
        let total = entries.len();
        let mut child = active.clone();
        for (index, (key, item)) in entries.iter().enumerate() {
            bind_loop_vars(
                &mut child,
                index,
                total,
                key.as_deref(),
                item,
            );
            // `#each` is the loop-control sink: Break stops iterating,
            // Continue and Done move on to the next item.
            if self.walk(frame.deeper(), body, &child, output)?
                == FlowSignal::Break
            {
                break;
            }
        }
        Ok(Step::Next(after_block))
    }

    /// Range form of `#each`: both sides are full expressions, so
    /// `#each 0..items.length` works. `..` is unambiguous because
    /// identifiers cannot contain consecutive dots.
    fn each_range(
        &self,
        origin: &str,
        arg: &str,
        active: &Context,
    ) -> Result<Option<Vec<Value>>, EngineError> {
        let Some((lo_str, hi_str)) = arg.split_once("..") else {
            return Ok(None);
        };
        let lo = self.eval_expr(origin, lo_str.trim(), arg, active)?;
        let hi = self.eval_expr(origin, hi_str.trim(), arg, active)?;
        match (lo, hi) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Some((a..b).map(Value::Number).collect()))
            }
            (l, h) => Err(EngineError::InvalidTemplate(format!(
                "#each range bounds must be numbers, got {l:?}..{h:?}{}",
                pos_suffix(origin, arg)
            ))),
        }
    }

    /// `{{ super() }}` inside a `#block` override renders the parent
    /// block's default body; elsewhere it emits nothing.
    fn tag_super<'t>(
        &self,
        frame: Frame<'t>,
        after: &'t str,
        context: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let Some(parent) = frame.super_body else {
            return Ok(Step::Next(after));
        };
        // super inside super is a no-op: the parent has no further
        // parent here.
        let inner = Frame {
            super_body: None,
            ..frame.deeper()
        };
        let signal = self.walk(inner, parent, context, output)?;
        Ok(propagate(signal, after))
    }

    /// `{{#block "name"}}default{{/block}}` renders the child's
    /// override if any (exposing the default via `super()`), otherwise
    /// the default body.
    fn tag_block<'t>(
        &self,
        frame: Frame<'t>,
        name_part: &str,
        tag: &Tag<'t>,
        context: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let name = parse_block_name(name_part)
            .map_err(|e| annotate_pos(e, frame.origin, tag.key))?;
        let (default_body, after_block) =
            extract_block(tag.after, "block", self.open, self.close)
                .map_err(|e| annotate_pos(e, frame.origin, tag.key))?;
        let (body, super_body) = match frame.blocks.get(name) {
            Some(over) => (over.as_str(), Some(default_body)),
            None => (default_body, None),
        };
        let inner = Frame {
            super_body,
            ..frame.deeper()
        };
        let signal = self.walk(inner, body, context, output)?;
        Ok(propagate(signal, after_block))
    }

    /// `{{> name}}` includes a partial with the current context;
    /// `{{> name k="v" n=7}}` layers bindings onto a clone of it.
    /// Errors inside the partial report positions in the partial's own
    /// source, and loop control does not leak across the boundary.
    fn tag_partial<'t>(
        &self,
        frame: Frame<'t>,
        call: &str,
        tag: &Tag<'t>,
        active: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let (name, params) = split_partial_invocation(call);
        if name.is_empty() {
            return Err(EngineError::InvalidTemplate(
                "Empty partial name".to_string(),
            ));
        }
        validate_name(name)?;
        let content = self.loader.load(name)?;
        let bound;
        let ctx = if params.is_empty() {
            active
        } else {
            let mut child = active.clone();
            for (k, v) in parse_partial_params(params)
                .map_err(|e| annotate_pos(e, frame.origin, tag.key))?
            {
                child.set_value(k, v);
            }
            bound = child;
            &bound
        };
        let inner = Frame {
            origin: &content,
            blocks: frame.blocks,
            super_body: None,
            depth: frame.depth + 1,
            escape: frame.escape,
        };
        let _ = self.walk(inner, &content, ctx, output)?;
        Ok(Step::Next(tag.after))
    }

    /// Plain substitution with an optional filter chain:
    /// `{{ title | uppercase }}`, `{{!raw}}`, `{{ html | safe }}`.
    fn tag_substitute<'t>(
        &self,
        frame: Frame<'t>,
        tag: &Tag<'t>,
        active: &Context,
        output: &mut String,
    ) -> Result<Step<'t>, EngineError> {
        let (lookup_raw, raw) = match tag.key.strip_prefix('!') {
            Some(stripped) => (stripped.trim_start(), true),
            None => (tag.key, false),
        };
        let mut parts = lookup_raw.split('|');
        let lookup = parts.next().unwrap_or("").trim();
        let filters: Vec<(String, Vec<String>)> = parts
            .map(parse_filter)
            .filter(|(name, _)| !name.is_empty())
            .collect();
        let Some(value) = active.get_path(lookup) else {
            // Issue #28: lax mode emits nothing for unresolved tags and
            // skips the filter chain.
            if self.lax_undefined {
                return Ok(Step::Next(tag.after));
            }
            return Err(EngineError::Render(format!(
                "Unresolved template tag: {lookup}{}",
                pos_suffix(frame.origin, lookup)
            )));
        };
        let rendered = self.apply_filters(value, &filters)?;
        // A trailing `safe` filter marks the value as already-safe HTML.
        let marked_safe =
            filters.last().is_some_and(|(name, _)| name == "safe");
        if raw || marked_safe || !frame.escape {
            output.push_str(&rendered);
        } else {
            escape_html_into(&rendered, output);
        }
        Ok(Step::Next(tag.after))
    }

    /// Runs the filter chain. Custom filters take precedence over
    /// built-ins; `json` (feature `json`) reads the structured value.
    fn apply_filters(
        &self,
        value: &Value,
        filters: &[(String, Vec<String>)],
    ) -> Result<String, EngineError> {
        let mut rendered = value.to_string();
        for (name, args) in filters {
            #[cfg(feature = "json")]
            if name == "json" {
                rendered = json_encode_value(value)?;
                continue;
            }
            rendered = match self.filters.and_then(|f| f.lookup(name)) {
                Some(custom) => custom(&rendered, args)?,
                None => apply_filter(name, args, rendered)?,
            };
        }
        Ok(rendered)
    }
}

/// `{{#set name = literal}}` binds `name` in the local scope visible to
/// subsequent tags at this depth (materialised from `context` on the
/// first set). The parent context is not mutated.
fn assign(
    origin: &str,
    tag: &Tag<'_>,
    assignment: &str,
    local: &mut Option<Context>,
    context: &Context,
) -> Result<(), EngineError> {
    let (name, value) = parse_set_assignment(assignment.trim())
        .map_err(|e| annotate_pos(e, origin, tag.key))?;
    local
        .get_or_insert_with(|| context.clone())
        .set_value(name, value);
    Ok(())
}

/// Copies the text before the delimiter at `start` into `output`,
/// halving the run of backslashes in front of it. Returns `true` when
/// the run is odd, i.e. the delimiter itself is escaped.
fn copy_text(rest: &str, start: usize, output: &mut String) -> bool {
    let bytes = rest.as_bytes();
    let mut bs = 0usize;
    while start > bs && bytes[start - bs - 1] == b'\\' {
        bs += 1;
    }
    output.push_str(&rest[..start - bs]);
    for _ in 0..bs / 2 {
        output.push('\\');
    }
    bs % 2 == 1
}

/// Whitespace control: `{{- … }}` strips trailing whitespace from
/// `output`, `{{ … -}}` skips leading whitespace after the tag. Block
/// comments `{{!-- … --}}` are exempt.
fn trim_tag<'t>(
    key_raw: &'t str,
    after_raw: &'t str,
    output: &mut String,
) -> Tag<'t> {
    let (key, strip_left, strip_right) =
        parse_ws_control(key_raw.trim());
    if strip_left {
        let kept = output.trim_end().len();
        output.truncate(kept);
    }
    let after = if strip_right {
        after_raw.trim_start()
    } else {
        after_raw
    };
    Tag { key, after }
}

/// Stray closing / else markers outside a block are an authoring error.
fn reject_stray_closer(
    origin: &str,
    key: &str,
) -> Result<(), EngineError> {
    if key.starts_with("/if")
        || key.starts_with("/each")
        || key == "else"
    {
        return Err(EngineError::InvalidTemplate(format!(
            "unexpected `{key}` outside a block{}",
            pos_suffix(origin, key)
        )));
    }
    Ok(())
}

/// `{{! … }}` and `{{!-- … --}}` are comments. `{{!key}}` (bang
/// followed by an identifier byte) keeps its raw-substitution meaning.
fn is_comment(key: &str) -> bool {
    key.strip_prefix('!').is_some_and(|after| {
        after.is_empty()
            || after.starts_with(char::is_whitespace)
            || (after.starts_with("--") && after.ends_with("--"))
    })
}

/// Lists iterate by position, maps by key (sorted, so iteration order
/// is deterministic whatever the map type).
fn each_entries<'c>(
    origin: &str,
    arg: &str,
    active: &'c Context,
    range: Option<&'c [Value]>,
) -> Result<Vec<(Option<String>, &'c Value)>, EngineError> {
    if let Some(items) = range {
        return Ok(items.iter().map(|v| (None, v)).collect());
    }
    let target = active.get_path(arg).ok_or_else(|| {
        EngineError::Render(format!(
            "#each: unresolved list `{arg}`{}",
            pos_suffix(origin, arg)
        ))
    })?;
    match target {
        Value::List(items) => {
            Ok(items.iter().map(|v| (None, v)).collect())
        }
        Value::Map(map) => Ok(sorted_entries(map)),
        other => Err(EngineError::InvalidTemplate(format!(
            "#each expects a list or map, got {other:?}"
        ))),
    }
}

/// Map entries sorted by key, so iteration order is deterministic
/// whatever the map type.
fn sorted_entries(map: &Map) -> Vec<(Option<String>, &Value)> {
    let mut keyed: Vec<_> = map.iter().collect();
    keyed.sort_by(|a, b| a.0.cmp(b.0));
    keyed
        .into_iter()
        .map(|(k, v)| (Some(k.clone()), v))
        .collect()
}

/// Binds `this`, `@index`, `@first`, `@last` and (for maps) `@key`.
/// `set_value_str` reuses the existing key slot after the first
/// iteration; `set_value_string` also reuses the String buffer.
fn bind_loop_vars(
    child: &mut Context,
    index: usize,
    total: usize,
    key: Option<&str>,
    item: &Value,
) {
    match item {
        Value::String(s) => child.set_value_string("this", s),
        other => child.set_value_str("this", other.clone()),
    }
    child.set_value_str(
        "@index",
        i64::try_from(index).unwrap_or(i64::MAX),
    );
    child.set_value_str("@first", index == 0);
    child.set_value_str("@last", index + 1 == total);
    if let Some(k) = key {
        child.set_value_str("@key", k);
    }
}
