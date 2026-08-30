use crate::error::spanned_report_on;
use crate::ir::{Arm, ArmPattern, EnvPair, Instruction, Segment, Template as IrTemplate};
use crate::syntax::source::ArmPattern as DslArmPattern;
use crate::syntax::{Part as DslPart, Template};
use std::collections::{BTreeMap, HashMap};

use super::CompileError;

/// Inline every `@(var)` reference in `tmpl` against `scope`, replacing each
/// with the (already-inlined) template it names. Commands are preserved as
/// `Cmd` parts — they are never executed here. Returns the flattened list of
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
                    return Err(CompileError::ValidationReport(vec![spanned_report_on(
                        format!("circular variable reference: {}", name),
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                    )]));
                }
                let var_tmpl = scope.get(name).ok_or_else(|| {
                    CompileError::ValidationReport(vec![spanned_report_on(
                        format!("undefined variable: {}", name),
                        sources,
                        source_name,
                        tmpl.offset,
                        tmpl.len.max(1),
                    )])
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
                    source_name: inner.source_name.clone(),
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
        source_name: tmpl.source_name.clone(),
    })
}

/// Lower an already-inlined DSL `Template` (no `Var` parts) into an IR
/// `Template`. Commands survive as `Cmd` nodes for the executor to execute.
pub(super) fn lower_template(tmpl: &Template) -> IrTemplate {
    IrTemplate {
        segments: tmpl
            .parts
            .iter()
            .map(|p| match p {
                DslPart::Lit(s) => Segment::Literal(s.clone()),
                DslPart::Var(_) => {
                    unreachable!("variables are inlined away before lowering")
                }
                DslPart::Cmd(inner) => Segment::Command(lower_template(inner)),
            })
            .collect(),
    }
}

/// Lower a function body into IR `Instruction`s, inlining every `@(var)`
/// reference (against `static_scope` plus function-local `bind`s) as it goes.
///
/// Function-local `bind x = T` adds `x -> T` to the local scope so later
/// references resolve to `T`; the `bind` itself becomes a plain command
/// execution (no `target`) since the variable is fully inlined at compile time.
/// Nested `env`/`switch` bodies get a *copy* of the local scope so their binds
/// do not leak into the surrounding body.
pub(super) fn lower_function_body(
    stmts: &[crate::syntax::FnStmt],
    static_scope: &BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut local_scope = static_scope.clone();
    lower_fn_stmts(stmts, &mut local_scope, sources, source_name)
}

fn lower_fn_stmts(
    stmts: &[crate::syntax::FnStmt],
    scope: &mut BTreeMap<String, Template>,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<Vec<Instruction>, CompileError> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::syntax::FnStmt::Log(t) => {
                out.push(Instruction::Log(lower_template(&inline_dsl_template(
                    t,
                    scope,
                    sources,
                    source_name,
                )?)));
            }
            crate::syntax::FnStmt::Bind { target, value } => {
                let inlined = inline_dsl_template(value, scope, sources, source_name)?;
                if let Some(name) = target {
                    scope.insert(name.clone(), inlined.clone());
                }
                out.push(Instruction::Exec {
                    value: lower_template(&inlined),
                });
            }
            crate::syntax::FnStmt::Cd(t) => {
                out.push(Instruction::Cd(lower_template(&inline_dsl_template(
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
                            value: lower_template(&inline_dsl_template(
                                &p.value,
                                scope,
                                sources,
                                source_name,
                            )?),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut inner = scope.clone();
                let body = lower_fn_stmts(body, &mut inner, sources, source_name)?;
                out.push(Instruction::Env { pairs, body });
            }
            crate::syntax::FnStmt::Switch { subject, arms } => {
                let subject =
                    lower_template(&inline_dsl_template(subject, scope, sources, source_name)?);
                let mut arms_out = Vec::new();
                for arm in arms {
                    let pattern = match &arm.pattern {
                        DslArmPattern::Lit(s) => ArmPattern::Lit(s.clone()),
                        DslArmPattern::Default => ArmPattern::Default,
                    };
                    let mut inner = scope.clone();
                    let body = lower_fn_stmts(&arm.body, &mut inner, sources, source_name)?;
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
