use crate::ir::{Arm, ArmPattern, EnvPair, Instruction, Segment, Template as IrTemplate};
use crate::syntax::source::ArmPattern as DslArmPattern;
use crate::syntax::{Part as DslPart, Template};
use std::collections::{BTreeMap, HashMap};

use super::CompileError;
use super::PendingFn;

/// What a `name(args);` call statement resolves against during lowering: the
/// named functions collected from every source file and the global
/// variables. Every function carries the source its statements were written
/// in, so diagnostics from an inlined body render against the right file.
pub(super) struct FnResolver<'a> {
    pub(super) globals: &'a BTreeMap<String, Template>,
    pub(super) functions: &'a BTreeMap<String, PendingFn>,
}

/// Lower a call to a named function: check the target exists and the
/// argument count matches, inline each argument against the caller's scope,
/// bind the results to the callee's params, and compile the callee's body
/// with `params -> globals` resolution. The call is spliced carbon-copy at
/// the call site; `cycle_stack` reports recursive call chains instead of
/// looping.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function_call(
    name: &str,
    args: &[Template],
    caller_scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    source_name: &str,
    offset: usize,
    len: usize,
    row: &mut usize,
) -> Result<Vec<Instruction>, CompileError> {
    let function = resolver.functions.get(name).ok_or_else(|| {
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

    // The callee sees its params and the global variables only: arguments
    // are inlined at the call site, so the caller's local bindings never
    // leak into the inlined body.
    let mut inner = resolver.globals.clone();
    for (param, arg) in function.params.iter().zip(args) {
        let value = inline_dsl_template(arg, caller_scope, sources, source_name)?;
        inner.insert(param.clone(), value);
    }
    cycle_stack.push(name.to_string());
    let lowered = compile_fn_stmts(
        &function.body,
        &mut inner,
        sources,
        &function.source_name,
        resolver,
        cycle_stack,
        row,
    )?;
    cycle_stack.pop();
    Ok(lowered)
}

/// Inline every `@(var)` reference in `tmpl` against `scope`, replacing each
/// with the (already-inlined) template it names. Commands are preserved as
/// `Cmd` parts -- they are never executed here. Returns the flattened list of
/// parts (a var that resolves to several parts is spliced in directly).
///
/// `stack` tracks the variable-resolution chain so a self- or mutually-referential
/// `var` is reported instead of looping forever.
pub(super) fn inline_dsl_parts(
    tmpl: &Template,
    scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
    stack: &mut Vec<String>,
) -> Result<Vec<DslPart>, CompileError> {
    let mut out = Vec::new();
    for part in &tmpl.parts {
        match part {
            DslPart::Lit(s) => out.push(DslPart::Lit(s.clone())),
            DslPart::Var(name) => {
                if stack.contains(name) {
                    return Err(super::error_in(
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                        format!("circular variable reference: {}", name),
                    ));
                }
                let var_tmpl = scope.get(name).ok_or_else(|| {
                    super::error_in(
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                        format!("undefined variable: {}", name),
                    )
                })?;
                stack.push(name.clone());
                let inlined = inline_dsl_parts(var_tmpl, scope, sources, source_name, stack)?;
                stack.pop();
                out.extend(inlined);
            }
            DslPart::Cmd(inner) => {
                let inlined = inline_dsl_parts(inner, scope, sources, source_name, stack)?;
                out.push(DslPart::Cmd(Template {
                    parts: inlined,
                    offset: inner.offset,
                    len: inner.len,
                }));
            }
        }
    }
    Ok(out)
}

/// Inline `@(var)` references in `tmpl` against `scope`, returning a template with
/// no `Var` parts.
pub(super) fn inline_dsl_template(
    tmpl: &Template,
    scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Template, CompileError> {
    let parts = inline_dsl_parts(tmpl, scope, sources, source_name, &mut Vec::new())?;
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
            })
            .collect(),
    }
}

/// Compile a function body's statements into IR `Instruction`s, inlining every
/// `@(var)` reference (against `scope` plus function-local `bind`s) as it goes.
///
/// Function-local `var x = T` maps name `x` to template `T` in the local
/// scope so later references resolve to `T`. The bind itself does NOT
/// emit an `Instruction`: execution is deferred to each use site via tolerant
/// `capture`. Only bare `$(cmd);` emits strict `Instruction::RunShellCmd`.
/// Nested `env`/`switch` bodies get a *copy* of the local scope so their binds
/// do not leak into the surrounding body.
///
/// A `name();` call statement is lowered carbon-copy: the target body (a
/// sibling project function first, then a global function) is compiled
/// recursively against a copy of the current scope and its instructions are
/// spliced in place. `cycle_stack` carries the chain of bodies being lowered,
/// so self- or mutually-recursive calls are reported instead of looping; the
/// `source_name` switches to the target's own source so diagnostics from an
/// inlined body render against the file it was written in.
/// Compile a body's statements into IR instructions, wrapping every
/// statement in a display `Step` with the next compile-assigned row.
///
/// This is the single body compiler: function bodies, run bodies, async
/// bodies, env bodies, and switch arms all lower through here, so the
/// display tree and the execution tree are the same structure. Calls are
/// flattened: the callee's statements are compiled (carbon-copy） as
/// sibling steps, not hidden under a header.
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_fn_stmts(
    stmts: &[crate::syntax::FnStmt],
    scope: &mut BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    row: &mut usize,
) -> Result<Vec<Instruction>, CompileError> {
    let mut out = Vec::new();
    for stmt in stmts {
        out.extend(compile_fn_stmt(
            stmt,
            scope,
            sources,
            source_name,
            resolver,
            cycle_stack,
            row,
        )?);
    }
    Ok(out)
}

/// Lower a single statement. Binds mutate `scope` (compile-time
/// substitution) and emit nothing; every other statement becomes one
/// display row, except calls, which flatten into their callee's rows.
#[allow(clippy::too_many_arguments)]
pub(super) fn compile_fn_stmt(
    stmt: &crate::syntax::FnStmt,
    scope: &mut BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
    resolver: &FnResolver<'_>,
    cycle_stack: &mut Vec<String>,
    row: &mut usize,
) -> Result<Vec<Instruction>, CompileError> {
    match stmt {
        crate::syntax::FnStmt::Log(t) => {
            let value = compile_template(&inline_dsl_template(t, scope, sources, source_name)?);
            Ok(vec![step(
                row,
                crate::ir::log_label(&plan_text(&value)),
                vec![Instruction::Log(value)],
            )])
        }
        crate::syntax::FnStmt::Exec(command) => {
            let value =
                compile_template(&inline_dsl_template(command, scope, sources, source_name)?);
            Ok(vec![step(
                row,
                crate::ir::exec_label(&plan_text(&value)),
                vec![Instruction::Exec { command: value }],
            )])
        }
        crate::syntax::FnStmt::Cd(t) => {
            let value = compile_template(&inline_dsl_template(t, scope, sources, source_name)?);
            Ok(vec![step(
                row,
                crate::ir::cd_label(&plan_text(&value)),
                vec![Instruction::Cd(value)],
            )])
        }
        crate::syntax::FnStmt::Bind { name, value } => {
            let inlined = inline_dsl_template(value, scope, sources, source_name)?;
            scope.insert(name.clone(), inlined);
            // Assignment bindings are fully inlined into scope.
            // No runtime command emitted: execution happens lazily at
            // each use site via tolerant capture.
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
            let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
            let index = take_row(row);
            let mut inner = scope.clone();
            let inner_body = compile_fn_stmts(
                body,
                &mut inner,
                sources,
                source_name,
                resolver,
                cycle_stack,
                row,
            )?;
            Ok(vec![Instruction::Step {
                row: index,
                label: crate::ir::env_label(&keys.join(", ")),
                body: vec![Instruction::Env {
                    pairs: ir_pairs,
                    body: inner_body,
                }],
            }])
        }
        crate::syntax::FnStmt::Async { body } => {
            let index = take_row(row);
            let mut inner = scope.clone();
            let inner_body = compile_fn_stmts(
                body,
                &mut inner,
                sources,
                source_name,
                resolver,
                cycle_stack,
                row,
            )?;
            Ok(vec![Instruction::Step {
                row: index,
                label: "async".to_string(),
                body: vec![Instruction::Async { body: inner_body }],
            }])
        }
        crate::syntax::FnStmt::Call {
            name,
            args,
            offset,
            len,
        } => {
            // Flattened: the callee's statements become sibling steps.
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
                row,
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
                row,
            )?;
            Ok(vec![Instruction::Context {
                project: compile_template(&inlined),
                body: inner_body,
            }])
        }
        crate::syntax::FnStmt::Switch { subject, arms } => {
            let subject =
                compile_template(&inline_dsl_template(subject, scope, sources, source_name)?);
            let mut arms_out = Vec::new();
            for arm in arms {
                let pattern = match &arm.pattern {
                    DslArmPattern::Default => ArmPattern::Default,
                    DslArmPattern::Template(template) => {
                        // Patterns match the resolved subject string, so only
                        // literal text is meaningful. `@(x)` inlines to its
                        // value here (becoming literal); `$(cmd)` has no
                        // static text and stays an error.
                        let inlined = inline_dsl_template(template, scope, sources, source_name)?;
                        let is_literal = inlined.parts.iter().all(|p| matches!(p, DslPart::Lit(_)));
                        if !is_literal {
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
                let arm_row = take_row(row);
                let mut inner = scope.clone();
                let body = compile_fn_stmts(
                    &arm.body,
                    &mut inner,
                    sources,
                    source_name,
                    resolver,
                    cycle_stack,
                    row,
                )?;
                arms_out.push(Arm {
                    row: arm_row,
                    pattern,
                    body,
                });
            }
            // The switch statement has no line of its own; each arm is a row.
            Ok(vec![Instruction::Switch {
                subject,
                arms: arms_out,
            }])
        }
    }
}

/// Wrap one statement's instructions in a display step with the next row.
fn step(row: &mut usize, label: String, body: Vec<Instruction>) -> Instruction {
    Instruction::Step {
        row: take_row(row),
        label,
        body,
    }
}

/// The next compile-assigned display row.
fn take_row(row: &mut usize) -> usize {
    let index = *row;
    *row += 1;
    index
}

/// The plan preview of a template: literal text with `$(...)` placeholders
/// kept visible (`@()` is already inlined at this point).
fn plan_text(template: &crate::ir::Template) -> String {
    crate::ir::template_plan_text(template)
}
