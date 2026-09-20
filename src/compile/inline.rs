use crate::ir::{
    ArmPattern, EnvPair, NodeId, NodeKind, ProgramBuilder, Segment, Template as IrTemplate,
};
use crate::syntax::source::ArmPattern as DslArmPattern;
use crate::syntax::{Part as DslPart, Template};
use std::collections::{BTreeMap, HashMap};

use super::CompileError;
use super::PendingFn;

/// What a `name(args);` call statement resolves against during lowering:
/// the named functions collected from every source file. Every function
/// carries the source its statements were written in, so diagnostics from an
/// inlined body render against the right file.
pub(super) struct FnResolver<'a> {
    /// Every definition of every name, in declaration order. A lookup picks
    /// the newest definition at or before the mentioning body's order.
    pub(super) functions: &'a BTreeMap<String, Vec<PendingFn>>,
}

/// What one name resolves to while lowering. A parameter is substituted: its
/// text is the caller's data, repeated wherever the parameter is used. A
/// variable is a tagged reference: the runtime computes the declaration's
/// value once and reuses the text for every reference.
#[derive(Debug, Clone)]
pub(super) enum Binding {
    Param(Template),
    Var {
        id: crate::ir::VarId,
        value: Template,
    },
}

/// Lower a call to a named function: resolve the target textually (the
/// newest definition at or before `bound`, the mentioning body's own
/// declaration order), inline each argument against the caller's scope, bind
/// the results to the callee's params, and compile the callee's body with
/// those parameters as its only scope and its own order as the new bound.
/// The call is spliced carbon-copy at the call site; `cycle_stack` reports
/// self-recursion instead of looping.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function_call(
    name: &str,
    args: &[Template],
    caller_scope: &BTreeMap<String, Binding>,
    sources: &HashMap<String, String>,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    source_name: &str,
    offset: usize,
    len: usize,
    bound: usize,
    next_var_id: &mut crate::ir::VarId,
    arena: &mut ProgramBuilder,
) -> Result<Vec<NodeId>, CompileError> {
    let function = resolver
        .functions
        .get(name)
        .and_then(|versions| versions.iter().rev().find(|f| f.epoch <= bound))
        .ok_or_else(|| {
            // At this point in the text the name is simply not declared yet;
            // a later declaration has not been read.
            super::error_in(
                sources,
                source_name,
                offset,
                len.max(1),
                format!("undefined function: `{name}`"),
            )
        })?;
    if function.params.len() != args.len() {
        return Err(super::error_in(
            sources,
            source_name,
            offset,
            len.max(1),
            format!(
                "function `{name}` takes {} argument(s), got {}",
                function.params.len(),
                args.len()
            ),
        ));
    }
    if cycle_stack.contains(&name.to_string()) {
        let mut chain = cycle_stack.join(" -> ");
        chain.push_str(" -> ");
        chain.push_str(name);
        return Err(super::error_in(
            sources,
            source_name,
            offset,
            len.max(1),
            format!("circular function call: {chain}"),
        ));
    }

    // The callee sees exactly its parameters (plus the binds it declares
    // itself): file variables and the caller's local bindings are not
    // visible, so data enters a function only through its call arguments.
    let mut inner = BTreeMap::new();
    for (param, arg) in function.params.iter().zip(args) {
        // Arguments are the caller's data: substitution, not a cached value.
        let value = inline_dsl_template(arg, caller_scope, sources, source_name)?;
        inner.insert(param.clone(), Binding::Param(value));
    }
    cycle_stack.push(name.to_string());
    let lowered = compile_fn_stmts(
        &function.body,
        &mut inner,
        sources,
        &function.source_name,
        resolver,
        cycle_stack,
        function.epoch,
        next_var_id,
        arena,
    )?;
    cycle_stack.pop();
    Ok(lowered)
}

/// Inline every `@(name)` reference in `tmpl` against `scope`. Parameters
/// are substituted (spliced), variables become tagged references carrying
/// the declaration's already-inlined value, so the runtime can compute once
/// and reuse. Commands stay as `Cmd` parts; nothing is executed here.
pub(super) fn inline_dsl_parts(
    tmpl: &Template,
    scope: &BTreeMap<String, Binding>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<DslPart>, CompileError> {
    let mut out = Vec::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push(DslPart::Lit(s.clone())),
            DslPart::Var(name) => match scope.get(name) {
                Some(Binding::Param(value)) => {
                    let inlined = inline_dsl_parts(value, scope, sources, source_name)?;
                    out.extend(inlined);
                }
                Some(Binding::Var { id, value }) => out.push(DslPart::Ref {
                    id: *id,
                    template: value.clone(),
                }),
                None => {
                    return Err(super::error_in(
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                        format!("undefined variable: {}", name),
                    ));
                }
            },
            DslPart::Cmd(inner) => {
                let inlined = inline_dsl_parts(inner, scope, sources, source_name)?;
                out.push(DslPart::Cmd(Template {
                    parts: inlined,
                    offset: inner.offset,
                    len: inner.len,
                }));
            }
            // Produced by this inliner already; its template is fully
            // inlined, so it passes through untouched.
            DslPart::Ref { .. } => out.push(part.clone()),
        }
    }
    Ok(out)
}

/// Inline `@(name)` references in `tmpl` against `scope`, returning a
/// template with no `Var` parts (parameters spliced, variables tagged).
pub(super) fn inline_dsl_template(
    tmpl: &Template,
    scope: &BTreeMap<String, Binding>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Template, CompileError> {
    let parts = inline_dsl_parts(tmpl, scope, sources, source_name)?;
    Ok(Template {
        parts,
        offset: tmpl.offset,
        len: tmpl.len,
    })
}

/// Lower an already-inlined DSL `Template` (no `Var` parts) into an IR
/// `Template`. Commands survive as `Cmd` nodes for the executor to execute.
pub(super) fn compile_template(tmpl: &Template) -> IrTemplate {
    IrTemplate {
        parts: tmpl
            .parts
            .iter()
            .map(|p| match p {
                DslPart::Lit(s) => Segment::Lit(s.clone()),
                DslPart::Var(_) => {
                    unreachable!("variables are inlined away before lowering")
                }
                DslPart::Cmd(inner) => Segment::Cmd(compile_template(inner)),
                DslPart::Ref { id, template } => Segment::Ref {
                    id: *id,
                    template: compile_template(template),
                },
            })
            .collect(),
    }
}

/// Compile a body's statements into program nodes, inlining every `@(var)`
/// reference (against `scope` plus function-local binds) as it goes.
///
/// This is the single body compiler: function bodies, run bodies, async
/// bodies, env bodies, switch arms, and project bodies all lower through
/// here, so the execution tree and the display tree are the same arena.
/// Calls are flattened: the callee's statements are compiled carbon-copy as
/// sibling nodes, not hidden under a header.
///
/// Function-local `var x = T` binds mutate `scope` and emit no node: each
/// declaration instance gets an identity, so the first use of a tagged
/// reference computes its value and every later use reuses it. Nested block
/// bodies get a copy of the local scope so their binds do not leak into the
/// surrounding body.
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_fn_stmts(
    stmts: &[crate::syntax::FnStmt],
    scope: &mut BTreeMap<String, Binding>,
    sources: &HashMap<String, String>,
    source_name: &str,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    bound: usize,
    next_var_id: &mut crate::ir::VarId,
    arena: &mut ProgramBuilder,
) -> Result<Vec<NodeId>, CompileError> {
    let mut out = Vec::new();
    for stmt in stmts {
        out.extend(compile_fn_stmt(
            stmt,
            scope,
            sources,
            source_name,
            resolver,
            cycle_stack,
            bound,
            next_var_id,
            arena,
        )?);
    }
    Ok(out)
}

/// Lower a single statement into one node (or, for calls, the callee's
/// flattened sibling nodes). Binds emit nothing.
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_fn_stmt(
    stmt: &crate::syntax::FnStmt,
    scope: &mut BTreeMap<String, Binding>,
    sources: &HashMap<String, String>,
    source_name: &str,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    bound: usize,
    next_var_id: &mut crate::ir::VarId,
    arena: &mut ProgramBuilder,
) -> Result<Vec<NodeId>, CompileError> {
    let leaf = |kind: NodeKind, arena: &mut ProgramBuilder| vec![arena.push(kind, Vec::new())];
    match stmt {
        crate::syntax::FnStmt::Log(t) => {
            let value = compile_template(&inline_dsl_template(t, scope, sources, source_name)?);
            Ok(leaf(NodeKind::Log(value), arena))
        }
        crate::syntax::FnStmt::Exec(command) => {
            let value =
                compile_template(&inline_dsl_template(command, scope, sources, source_name)?);
            Ok(leaf(NodeKind::Exec(value), arena))
        }
        crate::syntax::FnStmt::Cd(t) => {
            let value = compile_template(&inline_dsl_template(t, scope, sources, source_name)?);
            Ok(leaf(NodeKind::Cd(value), arena))
        }
        crate::syntax::FnStmt::Bind { name, value } => {
            let inlined = inline_dsl_template(value, scope, sources, source_name)?;
            // One identity per declaration instance: every later reference
            // in this body reuses the value the first use computes.
            let id = *next_var_id;
            *next_var_id += 1;
            scope.insert(name.clone(), Binding::Var { id, value: inlined });
            // Binds emit no node: the tagged references carry the value.
            Ok(Vec::new())
        }
        crate::syntax::FnStmt::EnvBlock { pairs, body } => {
            let ir_pairs = pairs
                .iter()
                .map(|p| -> Result<EnvPair, CompileError> {
                    Ok(EnvPair {
                        key: p.key.clone(),
                        value: compile_template(&inline_dsl_template(
                            &p.value,
                            scope,
                            sources,
                            source_name,
                        )?),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut inner = scope.clone();
            let inner_body = compile_fn_stmts(
                body,
                &mut inner,
                sources,
                source_name,
                resolver,
                cycle_stack,
                bound,
                next_var_id,
                arena,
            )?;
            Ok(vec![arena.push(NodeKind::Env(ir_pairs), inner_body)])
        }
        crate::syntax::FnStmt::Async { body } => {
            let mut inner = scope.clone();
            let inner_body = compile_fn_stmts(
                body,
                &mut inner,
                sources,
                source_name,
                resolver,
                cycle_stack,
                bound,
                next_var_id,
                arena,
            )?;
            Ok(vec![arena.push(NodeKind::Async, inner_body)])
        }
        crate::syntax::FnStmt::Call {
            name,
            args,
            offset,
            len,
        } => {
            // Flattened: the callee's statements become sibling nodes.
            lower_function_call(
                name,
                args,
                scope,
                sources,
                resolver,
                cycle_stack,
                source_name,
                *offset,
                *len,
                bound,
                next_var_id,
                arena,
            )
        }
        crate::syntax::FnStmt::Project { name, body } => {
            let inlined = inline_dsl_template(name, scope, sources, source_name)?;
            let mut inner = scope.clone();
            let inner_body = compile_fn_stmts(
                body,
                &mut inner,
                sources,
                source_name,
                resolver,
                cycle_stack,
                bound,
                next_var_id,
                arena,
            )?;
            Ok(vec![arena.push(
                NodeKind::Project(compile_template(&inlined)),
                inner_body,
            )])
        }
        crate::syntax::FnStmt::Switch { subject, arms } => {
            let subject =
                compile_template(&inline_dsl_template(subject, scope, sources, source_name)?);
            let mut arm_ids = Vec::new();
            for arm in arms {
                let pattern = match &arm.pattern {
                    DslArmPattern::Default => ArmPattern::Default,
                    DslArmPattern::Template(template) => {
                        // Patterns match the resolved subject string, so only
                        // literal text is meaningful. `@(x)` inlines to its
                        // value here (becoming literal); `$(cmd)` has no
                        // static text and stays an error.
                        let inlined = inline_dsl_template(template, scope, sources, source_name)?;
                        if !inlined.is_literal() {
                            return Err(super::error_in(
                                sources,
                                source_name,
                                template.offset,
                                template.len.max(1),
                                "case pattern must be literal text",
                            ));
                        }
                        ArmPattern::Lit(inlined.literal_text())
                    }
                };
                let mut inner = scope.clone();
                let body = compile_fn_stmts(
                    &arm.body,
                    &mut inner,
                    sources,
                    source_name,
                    resolver,
                    cycle_stack,
                    bound,
                    next_var_id,
                    arena,
                )?;
                arm_ids.push(arena.push(NodeKind::Arm(pattern), body));
            }
            // The switch has no row of its own; each arm child is a row.
            Ok(vec![arena.push(NodeKind::Switch(subject), arm_ids)])
        }
    }
}
