use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Arm, ArmPattern, EnvPair, Instruction, Segment, Template as IrTemplate};
use crate::syntax::source::ArmPattern as DslArmPattern;
use crate::syntax::{Part as DslPart, Template};
use std::collections::{BTreeMap, HashMap};

use super::CompileError;

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
                    return Err(CompileError::diagnostic(Diagnostic::new(
                        source_name.to_string(),
                        Span::new(tmpl.offset, tmpl.len.max(1)),
                        format!("circular variable reference: {}", name),
                        sources.get(source_name).cloned().unwrap_or_default(),
                    )));
                }
                let var_tmpl = scope.get(name).ok_or_else(|| {
                    CompileError::diagnostic(Diagnostic::new(
                        source_name.to_string(),
                        Span::new(tmpl.offset, tmpl.len.max(1)),
                        format!("undefined variable: {}", name),
                        sources.get(source_name).cloned().unwrap_or_default(),
                    ))
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

/// Compile a function body into IR `Instruction`s, inlining every `@(var)`
/// reference (against `static_scope` plus function-local `bind`s) as it goes.
///
/// Function-local `var x = T` maps name `x` to template `T` in the local
/// scope so later references resolve to `T`. The bind itself does NOT
/// emit an `Instruction`: execution is deferred to each use site via tolerant
/// `capture`. Only bare `$(cmd);` emits strict `Instruction::RunShellCmd`.
/// Nested `env`/`switch` bodies get a *copy* of the local scope so their binds
/// do not leak into the surrounding body.
pub(super) fn compile_function_body(
    stmts: &[crate::syntax::FnStmt],
    static_scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut local_scope = static_scope.clone();
    compile_fn_stmts(stmts, &mut local_scope, sources, source_name)
}

fn compile_fn_stmts(
    stmts: &[crate::syntax::FnStmt],
    scope: &mut BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::syntax::FnStmt::Log(t) => {
                out.push(Instruction::Log(compile_template(&inline_dsl_template(
                    t,
                    scope,
                    sources,
                    source_name,
                )?)));
            }
            crate::syntax::FnStmt::Bind { name, value } => {
                let inlined = inline_dsl_template(value, scope, sources, source_name)?;
                scope.insert(name.clone(), inlined);
                // Assignment bindings are fully inlined into scope.
                // No runtime command emitted: execution happens lazily at
                // each use site via tolerant capture (Switch/Log/Cd/Env).
            }
            crate::syntax::FnStmt::RunShellCmd(value) => {
                let ir =
                    compile_template(&inline_dsl_template(value, scope, sources, source_name)?);
                let has_cmd = ir
                    .parts
                    .iter()
                    .any(|s| matches!(s, crate::ir::Segment::Cmd(_)));
                if !has_cmd {
                    return Err(crate::compile::CompileError::diagnostic(Diagnostic::new(
                        source_name.to_string(),
                        Span::new(value.offset, value.len.max(1)),
                        "bare template is not a statement, wrap the command in $(...) or prefix with log, cd, var, env or switch",
                        sources.get(source_name).cloned().unwrap_or_default(),
                    )));
                }
                out.push(Instruction::RunShellCmd { value: ir });
            }
            crate::syntax::FnStmt::Cd(t) => {
                out.push(Instruction::Cd(compile_template(&inline_dsl_template(
                    t,
                    scope,
                    sources,
                    source_name,
                )?)));
            }
            crate::syntax::FnStmt::EnvBlock { pairs, body } => {
                let pairs = pairs
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
                let body = compile_fn_stmts(body, &mut inner, sources, source_name)?;
                out.push(Instruction::Env { pairs, body });
            }
            crate::syntax::FnStmt::Switch { subject, arms } => {
                let subject =
                    compile_template(&inline_dsl_template(subject, scope, sources, source_name)?);
                let mut arms_out = Vec::new();
                for arm in arms {
                    let pattern = match &arm.pattern {
                        DslArmPattern::Lit(s) => ArmPattern::Lit(s.clone()),
                        DslArmPattern::Default => ArmPattern::Default,
                    };
                    let mut inner = scope.clone();
                    let body = compile_fn_stmts(&arm.body, &mut inner, sources, source_name)?;
                    arms_out.push(Arm { pattern, body });
                }
                out.push(Instruction::Switch {
                    subject,
                    arms: arms_out,
                });
            }
        }
    }
    Ok(out)
}
